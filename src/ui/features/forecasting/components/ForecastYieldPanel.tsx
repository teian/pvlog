import { useForecastExport } from "@/features/forecasting/hooks/useForecasting";
import { ForecastMetadata } from "@/features/forecasting/components/ForecastMetadata";
import { ForecastTable } from "@/features/forecasting/components/ForecastTable";
import type { YieldSeries } from "@/features/forecasting/types/forecast.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  Skeleton,
} from "@/shared/components";
import { useReducedMotion } from "@/shared/hooks";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

/** Forward forecast panel properties. */
export interface ForecastYieldPanelProps {
  /** Active system. */ systemId: string;
  /** Forward modeled result. */ series?: YieldSeries;
  /** Query loading state. */ loading: boolean;
  /** Query failure state. */ error: boolean;
  /** Export range start. */ startEpochMillis: number;
  /** Export range end. */ endEpochMillis: number;
}

/** Renders a responsive forward forecast with uncertainty, metadata, summaries, table alternative, and matching exports. @param props - Query state, system, and range. @returns Forward forecast experience. */
export function ForecastYieldPanel({
  systemId,
  series,
  loading,
  error,
  startEpochMillis,
  endEpochMillis,
}: ForecastYieldPanelProps) {
  const { t, i18n } = useTranslation();
  const reducedMotion = useReducedMotion();
  const [table, setTable] = useState(false);
  const exportMutation = useForecastExport();
  if (loading) return <Skeleton className="h-96 w-full" />;
  if (error)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("forecasting.forward.errorTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.forward.errorDescription")}
        </AlertDescription>
      </Alert>
    );
  if (!series || series.points.length === 0)
    return (
      <Alert>
        <AlertTitle>{t("forecasting.forward.emptyTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.forward.emptyDescription")}
        </AlertDescription>
      </Alert>
    );
  const locale = i18n.resolvedLanguage ?? "en";
  const points = series.points.map((point) => ({
    timestamp: point.intervalStart,
    central:
      point.centralPowerWatts === null ? null : point.centralPowerWatts / 1000,
    lower: point.lowerPowerWatts === null ? null : point.lowerPowerWatts / 1000,
    upper: point.upperPowerWatts === null ? null : point.upperPowerWatts / 1000,
  }));
  const energy =
    series.points.reduce(
      (sum, point) => sum + (point.centralEnergyWattHours ?? 0),
      0,
    ) / 1000;
  const peak =
    Math.max(...series.points.map((point) => point.centralPowerWatts ?? 0)) /
    1000;
  const date = (value: number | null) =>
    value === null
      ? t("forecasting.notAvailable")
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(value);
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
            <CardTitle>{t("forecasting.forward.title")}</CardTitle>
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
        {series.freshness !== "fresh" || series.completeness !== "complete" ? (
          <Alert>
            <AlertTitle>{t("forecasting.forward.partialTitle")}</AlertTitle>
            <AlertDescription>
              {t("forecasting.forward.partialDescription")}
            </AlertDescription>
          </Alert>
        ) : null}
        <dl className="grid gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
          <ForecastMetadata
            label={t("forecasting.forward.energy")}
            value={t("forecasting.energyKwh", {
              value: energy.toLocaleString(locale, {
                maximumFractionDigits: 1,
              }),
            })}
          />
          <ForecastMetadata
            label={t("forecasting.forward.peak")}
            value={t("forecasting.powerKw", {
              value: peak.toLocaleString(locale, { maximumFractionDigits: 1 }),
            })}
          />
          <ForecastMetadata
            label={t("forecasting.forward.issued")}
            value={date(series.issueTime)}
          />
          <ForecastMetadata
            label={t("forecasting.forward.horizon")}
            value={`${date(series.points[0]?.intervalStart ?? null)} – ${date(series.points.at(-1)?.intervalEnd ?? null)}`}
          />
          <ForecastMetadata
            label={t("forecasting.forward.provider")}
            value={series.provenance.attribution}
          />
          <ForecastMetadata
            label={t("forecasting.forward.model")}
            value={`${series.modelIdentifier} v${String(series.modelRevision)}`}
          />
          <ForecastMetadata
            label={t("forecasting.forward.capacity")}
            value={t("forecasting.capacityKwp", {
              included: (series.includedCapacityWatts / 1000).toLocaleString(
                locale,
              ),
              total: (series.totalEffectiveCapacityWatts / 1000).toLocaleString(
                locale,
              ),
            })}
          />
          <ForecastMetadata
            label={t("forecasting.forward.configuration")}
            value={series.configurationDigest.slice(0, 12)}
          />
        </dl>
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
              exportData("csv");
            }}
            size="sm"
            variant="outline"
          >
            {t("forecasting.exportCsv")}
          </Button>
          <Button
            onClick={() => {
              exportData("json");
            }}
            size="sm"
            variant="outline"
          >
            {t("forecasting.exportJson")}
          </Button>
        </div>
        {table ? (
          <ForecastTable locale={locale} series={series} />
        ) : (
          <ChartContainer
            className="h-72 w-full"
            config={{
              central: {
                label: t("forecasting.forward.central"),
                color: "var(--chart-1)",
              },
              lower: {
                label: t("forecasting.forward.lower"),
                color: "var(--chart-2)",
              },
              upper: {
                label: t("forecasting.forward.upper"),
                color: "var(--chart-3)",
              },
            }}
          >
            <LineChart accessibilityLayer data={points}>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="timestamp"
                tickFormatter={(value: number) =>
                  new Intl.DateTimeFormat(locale, {
                    weekday: "short",
                    hour: "2-digit",
                  }).format(value)
                }
              />
              <YAxis tickFormatter={(value: number) => `${String(value)} kW`} />
              <ChartTooltip content={<ChartTooltipContent />} />
              <Line
                dataKey="central"
                dot={false}
                isAnimationActive={!reducedMotion}
                stroke="var(--color-central)"
              />
              <Line
                dataKey="lower"
                dot={false}
                isAnimationActive={!reducedMotion}
                stroke="var(--color-lower)"
                strokeDasharray="4 4"
              />
              <Line
                dataKey="upper"
                dot={false}
                isAnimationActive={!reducedMotion}
                stroke="var(--color-upper)"
                strokeDasharray="4 4"
              />
            </LineChart>
          </ChartContainer>
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
