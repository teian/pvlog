import type { SeriesUnit } from "@/features/charts/api/chartsApi";

/** Converts a canonical integer base-unit value into its display unit. @param unit - Canonical unit reported by the series. @param value - Raw integer value in the canonical base unit. @returns The value expressed in its display unit. */
export function convertSeriesValue(unit: SeriesUnit, value: number): number {
  switch (unit) {
    case "basis_points":
      return value / 100;
    case "milli_degrees_celsius":
      return value / 1000;
    default:
      return value;
  }
}

const UNIT_SYMBOLS: Record<SeriesUnit, string> = {
  watts: "W",
  watt_hours: "Wh",
  basis_points: "%",
  milli_degrees_celsius: "°C",
  integer: "",
};

/** Returns the short display symbol for a canonical series unit. @param unit - Canonical unit reported by the series. @returns The unit's display symbol, or an empty string for dimensionless values. */
export function seriesUnitSymbol(unit: SeriesUnit): string {
  return UNIT_SYMBOLS[unit];
}

/** Non-visual summary statistics for a series, used for screen-reader text and the accessible table. */
export interface SeriesSummary {
  /** Number of points in the series. */ count: number;
  /** Minimum value in display units. */ minimum: number;
  /** Maximum value in display units. */ maximum: number;
  /** Arithmetic mean value in display units. */ average: number;
}

/** Computes non-visual summary statistics for a converted-value series. @param values - Points already converted to their display unit. @returns Count, minimum, maximum, and average, or null when empty. */
export function computeSeriesSummary(values: number[]): SeriesSummary | null {
  if (values.length === 0) return null;
  const minimum = Math.min(...values);
  const maximum = Math.max(...values);
  const average = values.reduce((sum, value) => sum + value, 0) / values.length;
  return { count: values.length, minimum, maximum, average };
}
