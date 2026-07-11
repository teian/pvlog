import { describe, expect, it } from "vitest";

import { CHART_CATEGORIES } from "@/features/charts/utils/chartCategories";
import {
  convertSeriesValue,
  seriesUnitSymbol,
} from "@/features/charts/utils/formatSeriesValue";
import { formatTimestamp } from "@/features/charts/utils/formatTimestamp";
import { RANGE_PRESETS } from "@/features/charts/utils/rangePresets";

describe("convertSeriesValue", () => {
  it("converts basis points to a percentage", () => {
    expect(convertSeriesValue("basis_points", 9850)).toBeCloseTo(98.5);
  });

  it("converts milli-degrees Celsius to degrees", () => {
    expect(convertSeriesValue("milli_degrees_celsius", 21500)).toBeCloseTo(
      21.5,
    );
  });

  it("passes watts through unchanged", () => {
    expect(convertSeriesValue("watts", 4200)).toBe(4200);
  });
});

describe("seriesUnitSymbol", () => {
  it("returns a percent symbol for basis points", () => {
    expect(seriesUnitSymbol("basis_points")).toBe("%");
  });

  it("returns an empty symbol for dimensionless integers", () => {
    expect(seriesUnitSymbol("integer")).toBe("");
  });
});

describe("formatTimestamp", () => {
  const epochMillis = Date.parse("2026-03-15T13:45:00Z");

  it("includes a time component for sub-daily resolutions", () => {
    expect(formatTimestamp(epochMillis, "hourly", "UTC", "en")).toMatch(
      /13:45|1:45/,
    );
  });

  it("omits the time component for calendar resolutions", () => {
    expect(formatTimestamp(epochMillis, "daily", "UTC", "en")).not.toMatch(/:/);
  });

  it("returns an empty string for a non-finite timestamp instead of throwing", () => {
    expect(formatTimestamp(Number.NaN, "hourly", "UTC", "en")).toBe("");
  });
});

describe("chart configuration data", () => {
  it("exposes every category with at least one field", () => {
    expect(CHART_CATEGORIES.length).toBeGreaterThan(0);
    for (const category of CHART_CATEGORIES) {
      expect(category.fields.length).toBeGreaterThan(0);
    }
  });

  it("exposes range presets in ascending duration order", () => {
    const durations = RANGE_PRESETS.map((preset) => preset.durationMillis);
    expect(durations).toEqual([...durations].sort((a, b) => a - b));
  });
});
