import { PerformanceCardActions } from "@/features/forecasting/components/PerformanceCardActions";
import { PerformanceLineChart } from "@/features/forecasting/components/PerformanceLineChart";
import { PerformanceTable } from "@/features/forecasting/components/PerformanceTable";
import { useForecastExport } from "@/features/forecasting/hooks/useForecasting";
import type {
  ForecastRange,
  PerformanceSeries,
} from "@/features/forecasting/types/forecast.types";
import {
  averageRatioPercent,
  performanceChartPoints,
} from "@/features/forecasting/utils/performanceMetrics";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useReducedMotion } from "@/shared/hooks";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** One performance metric card properties. */
export interface PerformanceMetricCardProps {
  /** Active system. */ systemId: string;
  /** Aligned series. */ series?: PerformanceSeries;
  /** Bounded range. */ range: ForecastRange;
  /** Comparison definition. */ type:
    "generation_performance" | "forecast_realization";
}

/** Renders one explicitly named actual-versus-modeled comparison. @param props - System, range, metric, and aligned values. @returns Metric card. */
export function PerformanceMetricCard({
  systemId,
  series,
  range,
  type,
}: PerformanceMetricCardProps) {
  const { t, i18n } = useTranslation();
  const reducedMotion = useReducedMotion();
  const [table, setTable] = useState(false);
  const exportMutation = useForecastExport();
  const locale = i18n.resolvedLanguage ?? "en";
  const title = t(`forecasting.performance.${type}.title`);
  if (!series || series.points.length === 0)
    return (
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-semibold">{title}</CardTitle>
          <CardDescription>
            {t(`forecasting.performance.${type}.description`)}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground">
            {t("forecasting.performance.noData")}
          </p>
        </CardContent>
      </Card>
    );
  const averageRatio = averageRatioPercent(series);
  const chartPoints = performanceChartPoints(series);
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">{title}</CardTitle>
        <CardDescription>
          {t(`forecasting.performance.${type}.description`)}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
            {t("forecasting.performance.average")}
          </p>
          <p className="font-mono text-4xl font-bold text-primary tabular-nums">
            {averageRatio === null
              ? t("forecasting.notAvailable")
              : t("forecasting.percent", {
                  value: averageRatio.toLocaleString(locale, {
                    maximumFractionDigits: 1,
                  }),
                })}
          </p>
        </div>
        <p className="text-sm text-muted-foreground">
          {t(`forecasting.performance.${type}.explanation`)}
        </p>
        <PerformanceCardActions
          onExport={() => {
            exportMutation.mutate({
              systemId,
              range,
              field: type,
              format: "csv",
            });
          }}
          onViewChange={setTable}
          table={table}
        />
        {table ? (
          <PerformanceTable locale={locale} series={series} />
        ) : (
          <PerformanceLineChart
            locale={locale}
            points={chartPoints}
            reducedMotion={reducedMotion}
          />
        )}
      </CardContent>
    </Card>
  );
}
