import type { PerformanceSeries } from "@/features/forecasting/types/forecast.types";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Performance table properties. */
export interface PerformanceTableProps {
  /** Active locale. */ locale: string;
  /** Aligned series. */ series: PerformanceSeries;
}

/** Renders actual and modeled energy while preserving explicit unavailable values. @param props - Locale and aligned series. @returns Accessible performance table. */
export function PerformanceTable({ locale, series }: PerformanceTableProps) {
  const { t } = useTranslation();
  return (
    <Table>
      <TableCaption>{t("forecasting.performance.tableCaption")}</TableCaption>
      <TableHeader>
        <TableRow>
          <TableHead>{t("forecasting.interval")}</TableHead>
          <TableHead>{t("forecasting.performance.actual")}</TableHead>
          <TableHead>{t("forecasting.performance.modeled")}</TableHead>
          <TableHead>{t("forecasting.performance.ratio")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {series.points.map((point) => (
          <TableRow key={point.intervalStart}>
            <TableCell>
              {new Intl.DateTimeFormat(locale, { dateStyle: "short" }).format(
                point.intervalStart,
              )}
            </TableCell>
            <TableCell>
              {point.actualEnergyWattHours === null
                ? t("forecasting.notAvailable")
                : t("forecasting.energyKwh", {
                    value: (point.actualEnergyWattHours / 1000).toLocaleString(
                      locale,
                    ),
                  })}
            </TableCell>
            <TableCell>
              {point.modeledEnergyWattHours === null
                ? t("forecasting.notAvailable")
                : t("forecasting.energyKwh", {
                    value: (point.modeledEnergyWattHours / 1000).toLocaleString(
                      locale,
                    ),
                  })}
            </TableCell>
            <TableCell>
              {point.ratioBasisPoints === null
                ? t("forecasting.notAvailableReason", {
                    reason: t(
                      `forecasting.reasons.${point.unavailableReason ?? "missing_actual_telemetry"}`,
                    ),
                  })
                : t("forecasting.percent", {
                    value: (point.ratioBasisPoints / 100).toLocaleString(
                      locale,
                    ),
                  })}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
