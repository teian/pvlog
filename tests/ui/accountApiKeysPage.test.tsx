import { AccountApiKeyManager } from "@/features/accountApiKeys";
import i18n from "@/shared/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const id = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const secret = `pvlog_${id}.${"a".repeat(64)}`;
let fetchMock: ReturnType<typeof vi.fn>;
let keys: unknown[];

describe("AccountApiKeyManager", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.sessionStorage.setItem("pvlog.csrf-token", "csrf");
    keys = [];
    fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      if (init?.method === "POST") {
        const credential = metadata();
        keys = [credential];
        return json({ apiKey: secret, credential }, 201);
      }
      if (init?.method === "DELETE") {
        keys = [];
        return new Response(null, { status: 204 });
      }
      return json(keys);
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("creates an upload-only key and never puts its secret in query data", async () => {
    const user = userEvent.setup();
    const client = renderManager();
    await screen.findByText("No account API keys have been created yet.");
    await user.type(screen.getByLabelText("Name"), "Home uploader");
    await user.click(screen.getByLabelText("Upload PV data"));
    await user.click(screen.getByRole("button", { name: "Create API key" }));

    expect(await screen.findByDisplayValue(secret)).toBeVisible();
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/account/api-keys",
      expect.objectContaining({
        body: JSON.stringify({
          name: "Home uploader",
          scopes: ["telemetry:write"],
          expiresAtEpochMillis: null,
        }),
        method: "POST",
      }),
    );
    expect(
      JSON.stringify(client.getQueryData(["account", "api-keys"])),
    ).not.toContain(secret);
    await user.click(
      screen.getByRole("button", { name: "Close one-time API key" }),
    );
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument();
    expect(await screen.findByText("Home uploader")).toBeVisible();
  });

  it("explains missing permissions and revokes keys independently", async () => {
    keys = [metadata()];
    const user = userEvent.setup();
    renderManager();
    await screen.findByText("Home uploader");
    await user.click(screen.getByRole("button", { name: "Create API key" }));
    expect(screen.getByText("Enter a name for the API key.")).toBeVisible();
    expect(screen.getByText("Select at least one permission.")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Revoke" }));
    await user.click(screen.getByRole("button", { name: "Revoke API key" }));
    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        `/api/v1/account/api-keys/${id}`,
        expect.objectContaining({ method: "DELETE" }),
      );
    });
  });
});

function renderManager(): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <AccountApiKeyManager />
    </QueryClientProvider>,
  );
  return client;
}

function metadata() {
  return {
    id,
    name: "Home uploader",
    scopes: ["telemetry:write"],
    createdAtEpochMillis: 1_780_000_000_000,
    expiresAtEpochMillis: null,
    revokedAtEpochMillis: null,
  };
}

function json(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}
