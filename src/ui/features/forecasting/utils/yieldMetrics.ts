import type { YieldSeries } from "@/features/forecasting/types/forecast.types";

/** One forward forecast point, in kilowatts. */
export interface YieldChartPoint {
  /** Central estimate, or `null` where the model produced none. */ central:
    number | null;
  /** Lower uncertainty bound. */ lower: number | null;
  /** Interval start, as epoch milliseconds. */ timestamp: number;
  /** Upper uncertainty bound. */ upper: number | null;
}

/** Converts a forward series to chart points in kilowatts, preserving gaps as `null`. @param series - Forward modeled series. @returns Chart-ready points. */
export function yieldChartPoints(series: YieldSeries): YieldChartPoint[] {
  return series.points.map((point) => ({
    timestamp: point.intervalStart,
    central:
      point.centralPowerWatts === null ? null : point.centralPowerWatts / 1000,
    lower: point.lowerPowerWatts === null ? null : point.lowerPowerWatts / 1000,
    upper: point.upperPowerWatts === null ? null : point.upperPowerWatts / 1000,
  }));
}

/** Sums the forecast central energy over the range. @param series - Forward modeled series. @returns Total energy in kilowatt-hours. */
export function totalEnergyKwh(series: YieldSeries): number {
  const total = series.points.reduce(
    (sum, point) => sum + (point.centralEnergyWattHours ?? 0),
    0,
  );
  return total / 1000;
}

/** Finds the highest forecast central power over the range. @param series - Forward modeled series. @returns Peak power in kilowatts. */
export function peakPowerKw(series: YieldSeries): number {
  const watts = series.points.map((point) => point.centralPowerWatts ?? 0);
  return Math.max(...watts) / 1000;
}
