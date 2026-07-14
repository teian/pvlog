import type { YieldSeries } from "@/features/forecasting/types/forecast.types";
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

/** Forward forecast table properties. */
export interface ForecastTableProps {
  /** Active locale. */ locale: string;
  /** Modeled series. */ series: YieldSeries;
}

/** Renders a localized, accessible forecast table preserving unavailable values. @param props - Locale and modeled series. @returns Forecast table. */
export function ForecastTable({ locale, series }: ForecastTableProps) {
  const { t } = useTranslation();
  return (
    <Table>
      <TableCaption>{t("forecasting.forward.tableCaption")}</TableCaption>
      <TableHeader>
        <TableRow>
          <TableHead>{t("forecasting.interval")}</TableHead>
          <TableHead>{t("forecasting.forward.central")}</TableHead>
          <TableHead>{t("forecasting.forward.uncertainty")}</TableHead>
          <TableHead>{t("forecasting.coverage")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {series.points.map((point) => (
          <TableRow key={point.intervalStart}>
            <TableCell>
              {new Intl.DateTimeFormat(locale, {
                dateStyle: "short",
                timeStyle: "short",
              }).format(point.intervalStart)}
            </TableCell>
            <TableCell className="tabular-nums">
              {point.centralPowerWatts === null
                ? t("forecasting.notAvailable")
                : t("forecasting.powerKw", {
                    value: (point.centralPowerWatts / 1000).toLocaleString(
                      locale,
                    ),
                  })}
            </TableCell>
            <TableCell className="tabular-nums">
              {point.lowerPowerWatts === null || point.upperPowerWatts === null
                ? t("forecasting.notAvailable")
                : `${(point.lowerPowerWatts / 1000).toLocaleString(locale)}–${(point.upperPowerWatts / 1000).toLocaleString(locale)} kW`}
            </TableCell>
            <TableCell>
              {t("forecasting.percent", {
                value: (point.coverageBasisPoints / 100).toLocaleString(locale),
              })}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
