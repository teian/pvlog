import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { DataSourceManager } from "@/features/dataSources";
import i18n from "@/shared/lib/i18n";

const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const sourceId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const importId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f74";
const imports = `/api/v1/systems/${systemId}/data-sources/${sourceId}/imports`;

function json(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: {
      "content-type":
        status < 400 ? "application/json" : "application/problem+json",
    },
  });
}

const report = {
  rows: 16,
  accepted: 9,
  rejectedCount: 7,
  rejected: [
    { row: 8, reason: "invalid_date", column: "date" },
    { row: 11, reason: "nonexistent_local_time", column: "time" },
  ],
  firstDate: "2027-01-15",
  lastDate: "2027-10-31",
  overlap: { alreadyImported: 0, otherSources: 1 },
};

function dryRun(state: "ready" | "committed") {
  return {
    id: importId,
    dataSourceId: sourceId,
    fileName: "status.csv",
    kind: "status",
    state,
    createdAtEpochMillis: 1_800_000_000_000,
    ...(state === "ready"
      ? { expiresAtEpochMillis: 1_800_086_400_000 }
      : {
          committedAtEpochMillis: 1_800_000_060_000,
          result: { stored: 8, alreadyPresent: 0, skipped: 1 },
        }),
    report,
  };
}

interface Upload {
  encoding: string | null;
  body: unknown;
}

async function readUpload(init: RequestInit | undefined): Promise<Upload> {
  const encoding = new Headers(init?.headers).get("content-encoding");
  const raw = new Response(init?.body ?? null).body;
  const stream =
    encoding === "gzip" && raw
      ? raw.pipeThrough(new DecompressionStream("gzip"))
      : raw;
  return { encoding, body: JSON.parse(await new Response(stream).text()) };
}

function installBackend(dryRunStatus = 201) {
  const backend = { uploads: [] as Upload[], commits: 0 };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : input.toString();
      const method = init?.method ?? "GET";
      if (url === "/api/v1/data-source-profiles") return json([]);
      if (url === `/api/v1/systems/${systemId}/data-sources`)
        return json([
          {
            id: sourceId,
            systemId,
            name: "PVOutput history",
            profile: "import",
            state: "never_connected",
            serverUrl: "https://pvlog.example",
            credentialId: null,
            lastReadingAtEpochMillis: null,
            lastReceivedAtEpochMillis: null,
            readingsLastDay: 0,
            createdAtEpochMillis: 1,
            updatedAtEpochMillis: 1,
          },
        ]);
      if (url === imports && method === "GET")
        return json(backend.commits > 0 ? [dryRun("committed")] : []);
      if (url === imports && method === "POST") {
        backend.uploads.push(await readUpload(init));
        return dryRunStatus === 201
          ? json(dryRun("ready"), 201)
          : json(
              { title: "file_too_large", status: 413, limit: 4_194_304 },
              413,
            );
      }
      if (url === `${imports}/${importId}/commit`) {
        backend.commits += 1;
        return json(dryRun("committed"));
      }
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderManager(readOnly = false) {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <DataSourceManager readOnly={readOnly} systemId={systemId} />
    </QueryClientProvider>,
  );
}

const statusFile = () =>
  new File(["Date,Time,Energy\n20270115,08:00,10"], "status.csv", {
    type: "text/csv",
  });

describe("history import", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("stores nothing until the dry run is committed", async () => {
    const backend = installBackend();
    const user = userEvent.setup();
    renderManager();

    const start = await screen.findByRole("button", { name: "Start dry run" });
    expect(start).toBeDisabled();
    await user.upload(screen.getByLabelText("Export file (CSV)"), statusFile());
    await user.click(start);

    // The dry run reports what the file holds and why rows were rejected.
    const result = await screen.findByRole("region", {
      name: "Dry run result",
    });
    expect(
      within(result).getByText("Dry run of “status.csv”: nothing stored yet."),
    ).toBeVisible();
    expect(within(result).getByText("2027-01-15 to 2027-10-31")).toBeVisible();
    expect(within(result).getByText("7 rejected rows")).toBeInTheDocument();
    expect(
      within(result).getByText(
        "Row 11: time does not exist because the clocks go forward (column time)",
      ),
    ).toBeInTheDocument();
    expect(backend.commits).toBe(0);
    // The file travels compressed, as text in JSON.
    expect(backend.uploads).toEqual([
      {
        encoding: "gzip",
        body: {
          fileName: "status.csv",
          content: "Date,Time,Energy\n20270115,08:00,10",
        },
      },
    ]);

    await user.click(
      within(result).getByRole("button", { name: "Commit 9 rows" }),
    );
    expect(
      await within(result).findByText("“status.csv” was imported."),
    ).toBeVisible();
    expect(backend.commits).toBe(1);
    expect(within(result).getByText("Skipped").nextSibling).toHaveTextContent(
      "1",
    );
    expect(
      within(result).queryByRole("button", { name: /Commit/ }),
    ).not.toBeInTheDocument();
  });

  it("refuses a file over the limit and names the limit", async () => {
    const backend = installBackend(413);
    const user = userEvent.setup();
    renderManager();

    const input = await screen.findByLabelText("Export file (CSV)");
    await user.upload(
      input,
      new File(["1".repeat(4 * 1024 * 1024 + 1)], "huge.csv"),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("larger than 4 MiB");
    expect(
      screen.getByRole("button", { name: "Start dry run" }),
    ).toBeDisabled();
    expect(backend.uploads).toHaveLength(0);

    // When the server refuses a file, the limit is named as well.
    await user.upload(input, statusFile());
    await user.click(screen.getByRole("button", { name: "Start dry run" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "larger than 4 MiB",
    );
  });

  it("offers no import to someone who may only read", async () => {
    installBackend();
    renderManager(true);

    expect(await screen.findByText("PVOutput history")).toBeVisible();
    expect(
      screen.queryByLabelText("Export file (CSV)"),
    ).not.toBeInTheDocument();
  });
});
