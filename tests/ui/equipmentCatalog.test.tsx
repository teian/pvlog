import { InverterCatalogSelector } from "@/features/equipmentCatalog";
import i18n from "@/shared/lib/i18n";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const entry = {
  id: "test-inverter",
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

function renderSelector(onManual = vi.fn(), onSelect = vi.fn()) {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <InverterCatalogSelector onManual={onManual} onSelect={onSelect} />
    </QueryClientProvider>,
  );
  return { onManual, onSelect };
}

describe("equipment catalog selector", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("supports keyboard selection while retaining manual entry", async () => {
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
              items: [entry],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      ),
    );
    const callbacks = renderSelector();
    const results = await screen.findByLabelText("Matching templates");
    results.focus();
    expect(results).toHaveFocus();
    await userEvent.selectOptions(results, "test-inverter");
    expect(callbacks.onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: "test-inverter" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Enter inverter manually" }),
    );
    expect(callbacks.onManual).toHaveBeenCalledOnce();
  });

  it("keeps manual entry available when search has no results", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              revision: "2026.1",
              total: 0,
              offset: 0,
              limit: 25,
              items: [],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          ),
      ),
    );
    renderSelector();
    expect(await screen.findByText(/No matching catalog entry/)).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Enter inverter manually" }),
    ).toBeEnabled();
  });
});
