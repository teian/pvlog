import { InverterDraftEditor } from "@/features/systemManagement/components/InverterDraftEditor";
import { SystemWizard } from "@/features/systemManagement/components/SystemWizard";
import type { SystemInverterDraft } from "@/features/systemManagement/types/systemManagement.types";
import { emptyInverter } from "@/features/systemManagement/utils/systemManagementDraft";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import i18n from "@/shared/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

function InverterEditorHarness() {
  const [inverter, setInverter] = useState<SystemInverterDraft>(() =>
    emptyInverter(),
  );
  return (
    <QueryClientProvider client={new QueryClient()}>
      <InverterDraftEditor
        canRemove={false}
        index={0}
        onChange={setInverter}
        onRemove={vi.fn()}
        value={inverter}
      />
    </QueryClientProvider>
  );
}

describe("manual inverter editor", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("de");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              revision: "test",
              total: 0,
              offset: 0,
              limit: 25,
              items: [],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      ),
    );
  });

  it("explains why saving is disabled and omits the redundant back action", async () => {
    const user = userEvent.setup();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <SystemWizard
          error={null}
          onCancel={vi.fn()}
          onSubmit={vi.fn(async () => undefined)}
          pending={false}
        />
      </QueryClientProvider>,
    );

    const createSystem = screen.getByRole("button", {
      name: "Anlage anlegen",
    });
    expect(createSystem).toBeDisabled();
    expect(
      screen.getByText("Speichern nicht möglich: Der Anlagenname fehlt."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Zurück" }),
    ).not.toBeInTheDocument();

    await user.type(screen.getByLabelText("Anlagenname"), "Hausdach");

    expect(createSystem).toBeEnabled();
    expect(
      screen.getByText("Alle Änderungen werden gemeinsam gespeichert."),
    ).toBeInTheDocument();
  });

  it("explains a server-side validation rejection", () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <SystemWizard
          error={new SessionRequestError(422, null, null)}
          onCancel={vi.fn()}
          onSubmit={vi.fn(async () => undefined)}
          pending={false}
        />
      </QueryClientProvider>,
    );

    expect(
      screen.getByText(
        "Der Server hat die Anlagendaten abgelehnt. Prüfe Anlagenname und Zeitzone und versuche es erneut.",
      ),
    ).toBeVisible();
  });

  it("does not expose unused MPPT and phase inputs", () => {
    render(<InverterEditorHarness />);

    expect(screen.queryByLabelText("MPPTs")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Phasen")).not.toBeInTheDocument();
  });

  it("configures PV strings inside the inverter editor", async () => {
    const user = userEvent.setup();
    render(<InverterEditorHarness />);

    expect(screen.getByRole("heading", { name: "PV-Strings" })).toBeVisible();
    expect(screen.getAllByLabelText("String-Bezeichnung")).toHaveLength(1);

    await user.click(
      screen.getByRole("button", {
        name: "String zu Wechselrichter 1 hinzufügen",
      }),
    );

    expect(screen.getAllByLabelText("String-Bezeichnung")).toHaveLength(2);
  });

  it("accepts a rated power without retaining the default zero", async () => {
    const user = userEvent.setup();
    render(<InverterEditorHarness />);

    const ratedPower = screen.getByLabelText("Max. Leistung (W)");
    expect(ratedPower).toHaveValue(null);

    await user.type(ratedPower, "5000");

    expect(ratedPower).toHaveValue(5000);
  });

  it("allows panel count and module power to be cleared and overwritten", async () => {
    const user = userEvent.setup();
    render(<InverterEditorHarness />);

    const panelCount = screen.getByLabelText("Module");
    const modulePower = screen.getByLabelText("Leistung (Wp)");

    await user.clear(panelCount);
    expect(panelCount).toHaveValue(null);
    await user.type(panelCount, "18");
    expect(panelCount).toHaveValue(18);

    await user.clear(modulePower);
    expect(modulePower).toHaveValue(null);
    await user.type(modulePower, "180");
    expect(modulePower).toHaveValue(180);
  });
});
