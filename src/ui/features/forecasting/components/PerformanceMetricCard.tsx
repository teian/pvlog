import { PerformanceTable } from "@/features/forecasting/components/PerformanceTable";
import { useForecastExport } from "@/features/forecasting/hooks/useForecasting";
import type {
  ForecastRange,
  PerformanceSeries,
} from "@/features/forecasting/types/forecast.types";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/shared/components";
import { useReducedMotion } from "@/shared/hooks";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

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
  const available = series.points.filter(
    (point) => point.ratioBasisPoints !== null,
  );
  const averageRatio =
    available.length === 0
      ? null
      : available.reduce(
          (sum, point) => sum + (point.ratioBasisPoints ?? 0),
          0,
        ) /
        available.length /
        100;
  const chartPoints = series.points.map((point) => ({
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
        <div className="flex flex-wrap gap-2">
          <Button
            aria-pressed={!table}
            onClick={() => {
              setTable(false);
            }}
            size="sm"
            variant={!table ? "default" : "outline"}
          >
            {t("forecasting.view.chart")}
          </Button>
          <Button
            aria-pressed={table}
            onClick={() => {
              setTable(true);
            }}
            size="sm"
            variant={table ? "default" : "outline"}
          >
            {t("forecasting.view.table")}
          </Button>
          <Button
            onClick={() => {
              exportMutation.mutate({
                systemId,
                range,
                field: type,
                format: "csv",
              });
            }}
            size="sm"
            variant="outline"
          >
            {t("forecasting.exportCsv")}
          </Button>
        </div>
        {table ? (
          <PerformanceTable locale={locale} series={series} />
        ) : (
          <ChartContainer
            className="h-64 w-full"
            config={{
              actual: {
                label: t("forecasting.performance.actual"),
                color: "var(--chart-1)",
              },
              modeled: {
                label: t("forecasting.performance.modeled"),
                color: "var(--chart-3)",
              },
            }}
          >
            <LineChart accessibilityLayer data={chartPoints}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="timestamp"
                tickFormatter={(value: number) =>
                  new Intl.DateTimeFormat(locale, {
                    dateStyle: "short",
                  }).format(value)
                }
              />
              <YAxis
                tickFormatter={(value: number) => `${String(value)} kWh`}
              />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Line
                dataKey="actual"
                dot={false}
                isAnimationActive={!reducedMotion}
                stroke="var(--color-actual)"
              />
              <Line
                dataKey="modeled"
                dot={false}
                isAnimationActive={!reducedMotion}
                stroke="var(--color-modeled)"
                strokeDasharray="4 4"
              />
            </LineChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  );
}
