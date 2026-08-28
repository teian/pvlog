import { ForecastTable } from "@/features/forecasting/components/ForecastTable";
import { ForecastYieldActions } from "@/features/forecasting/components/ForecastYieldActions";
import { ForecastYieldChart } from "@/features/forecasting/components/ForecastYieldChart";
import { ForecastYieldMetadata } from "@/features/forecasting/components/ForecastYieldMetadata";
import { useForecastExport } from "@/features/forecasting/hooks/useForecasting";
import type { YieldSeries } from "@/features/forecasting/types/forecast.types";
import {
  peakPowerKw,
  totalEnergyKwh,
  yieldChartPoints,
} from "@/features/forecasting/utils/yieldMetrics";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useReducedMotion } from "@/shared/hooks";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Loaded forward forecast card properties. */
export interface ForecastYieldCardProps {
  /** Export range end. */ endEpochMillis: number;
  /** Forward modeled result, known to hold at least one point. */ series: YieldSeries;
  /** Export range start. */ startEpochMillis: number;
  /** Active system. */ systemId: string;
}

/** Renders the loaded forward forecast: freshness, metadata, chart or table, exports, and a text summary. @param props - System, range, and the loaded series. @returns The forward forecast card. */
export function ForecastYieldCard({
  endEpochMillis,
  series,
  startEpochMillis,
  systemId,
}: ForecastYieldCardProps) {
  const { t, i18n } = useTranslation();
  const reducedMotion = useReducedMotion();
  const [table, setTable] = useState(false);
  const exportMutation = useForecastExport();
  const locale = i18n.resolvedLanguage ?? "en";
  const energy = totalEnergyKwh(series);
  const peak = peakPowerKw(series);
  const degraded =
    series.freshness !== "fresh" || series.completeness !== "complete";
  const exportData = (format: "csv" | "json") => {
    exportMutation.mutate({
      systemId,
      range: {
        startEpochMillis,
        endEpochMillis,
        resolution: "hour",
        maximumPoints: 1000,
      },
      field: "forecast_power",
      format,
    });
  };
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle className="text-sm font-semibold">
              {t("forecasting.forward.title")}
            </CardTitle>
            <CardDescription>
              {t("forecasting.forward.description")}
            </CardDescription>
          </div>
          <Badge variant={series.freshness === "fresh" ? "default" : "outline"}>
            {t(`forecasting.freshness.${series.freshness}`)}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {degraded ? (
          <Alert>
            <AlertTitle>{t("forecasting.forward.partialTitle")}</AlertTitle>
            <AlertDescription>
              {t("forecasting.forward.partialDescription")}
            </AlertDescription>
          </Alert>
        ) : null}
        <ForecastYieldMetadata
          energy={energy}
          locale={locale}
          peak={peak}
          series={series}
        />
        <ForecastYieldActions
          onExport={exportData}
          onViewChange={setTable}
          table={table}
        />
        {table ? (
          <ForecastTable locale={locale} series={series} />
        ) : (
          <ForecastYieldChart
            locale={locale}
            points={yieldChartPoints(series)}
            reducedMotion={reducedMotion}
          />
        )}
        <p className="text-sm text-muted-foreground">
          {t("forecasting.forward.textSummary", {
            energy: energy.toLocaleString(locale, { maximumFractionDigits: 1 }),
            peak: peak.toLocaleString(locale, { maximumFractionDigits: 1 }),
            attribution: series.provenance.attribution,
          })}
        </p>
      </CardContent>
    </Card>
  );
}
