import { InverterCatalogField } from "@/features/systemManagement/components/InverterCatalogField";
import i18n from "@/shared/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const inverter = {
  id: "sma-sunny-test",
  revision: "2026.1",
  manufacturer: "SMA",
  model: "Sunny Test",
  provenance: {
    sourceName: "SMA",
    sourceReference: "https://example.test/sma",
  },
  dc: {
    topology: null,
    totalStringInputCount: 2,
    maximumInputVoltageMillivolts: null,
    mpptInputs: [
      {
        trackerIndex: 1,
        stringInputCount: 2,
        maximumOperatingCurrentMilliamperes: null,
      },
    ],
  },
  ac: { phaseCount: 3, ratedActivePowerWatts: 10_000 },
  operational: {
    maximumEfficiencyBasisPoints: null,
    operatingTemperature: null,
    communicationInterfaces: [],
    dimensionsMillimetres: null,
    weightGrams: null,
  },
};

function renderField(onSelect = vi.fn()) {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <InverterCatalogField
        id="inverter-catalog"
        onSelect={onSelect}
        value=""
      />
    </QueryClientProvider>,
  );
  return onSelect;
}

describe("system management catalog combobox", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("de");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              revision: "2026.1",
              total: 1,
              offset: 0,
              limit: 25,
              items: [inverter],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      ),
    );
  });

  it("opens a searchable design-style popup with manual and catalog options", async () => {
    const user = userEvent.setup();
    const onSelect = renderField();

    await user.click(
      screen.getByRole("combobox", { name: "Wechselrichterkatalog" }),
    );

    expect(screen.getByText("Manuell eingeben")).toBeVisible();
    expect(screen.getByText("Kenndaten selbst erfassen")).toBeVisible();
    expect(await screen.findByText("SMA Sunny Test")).toBeVisible();
    expect(screen.getByText("10000 W · 1 MPPT · 3-phasig")).toBeVisible();

    const search = screen.getByRole("combobox", {
      name: "Hersteller oder Typ suchen",
    });
    await user.type(search, "Sunny");
    await waitFor(() => {
      expect(fetch).toHaveBeenCalledWith(
        expect.stringContaining("search=Sunny"),
        expect.any(Object),
      );
    });

    await user.click(screen.getByText("SMA Sunny Test"));
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: "sma-sunny-test" }),
      "SMA Sunny Test",
    );
  });

  it("selects manual entry and clears the model value", async () => {
    const user = userEvent.setup();
    const onSelect = renderField();

    await user.click(
      screen.getByRole("combobox", { name: "Wechselrichterkatalog" }),
    );
    await user.click(screen.getByText("Manuell eingeben"));

    expect(onSelect).toHaveBeenCalledWith(null, "");
    expect(
      screen.getByRole("combobox", { name: "Wechselrichterkatalog" }),
    ).toHaveTextContent("Manuell eingeben");
  });
});
