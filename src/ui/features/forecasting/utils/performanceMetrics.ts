import type { PerformanceSeries } from "@/features/forecasting/types/forecast.types";

/** One actual-versus-modeled point, in kilowatt-hours. */
export interface PerformanceChartPoint {
  /** Measured energy, or `null` where no measurement exists. */ actual:
    number | null;
  /** Modeled energy, or `null` where the model produced none. */ modeled:
    number | null;
  /** Interval start, as epoch milliseconds. */ timestamp: number;
}

/** Averages the available actual-versus-modeled ratios as a percentage. @param series - Aligned performance series. @returns The mean ratio in percent, or `null` when no interval carries one. */
export function averageRatioPercent(series: PerformanceSeries): number | null {
  const available = series.points.filter(
    (point) => point.ratioBasisPoints !== null,
  );
  if (available.length === 0) return null;
  const total = available.reduce(
    (sum, point) => sum + (point.ratioBasisPoints ?? 0),
    0,
  );
  return total / available.length / 100;
}

/** Converts a performance series to chart points in kilowatt-hours, preserving gaps as `null`. @param series - Aligned performance series. @returns Chart-ready points. */
export function performanceChartPoints(
  series: PerformanceSeries,
): PerformanceChartPoint[] {
  return series.points.map((point) => ({
    timestamp: point.intervalStart,
    actual:
      point.actualEnergyWattHours === null
        ? null
        : point.actualEnergyWattHours / 1000,
    modeled:
      point.modeledEnergyWattHours === null
        ? null
        : point.modeledEnergyWattHours / 1000,
  }));
}
