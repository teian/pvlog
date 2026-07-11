import type {
  ResolutionParam,
  SeriesField,
} from "@/features/charts/api/chartsApi";
import { SeriesLineChart } from "@/features/charts/components/SeriesLineChart";
import { useSeries } from "@/features/charts/hooks/useSeries";
import type {
  ChartCategoryDefinition,
  ChartCategoryKey,
} from "@/features/charts/utils/chartCategories";
import {
  convertSeriesValue,
  seriesUnitSymbol,
} from "@/features/charts/utils/formatSeriesValue";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
  ToggleGroup,
  ToggleGroupItem,
} from "@/shared/components";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";

const CATEGORY_COLORS: Record<ChartCategoryKey, string> = {
  generation: "var(--chart-1)",
  consumption: "var(--chart-2)",
  grid: "var(--chart-3)",
  battery: "var(--chart-4)",
  environment: "var(--chart-5)",
  extended: "var(--chart-1)",
};

/** Historical chart properties for one category. */
export interface CategoryChartProps {
  /** System whose telemetry is displayed. */ systemId: string;
  /** Category definition and its selectable fields. */
  category: ChartCategoryDefinition;
  /** Range length ending now, in milliseconds. */ durationMillis: number;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
}

function averageCoveragePercent(
  points: { coverageBasisPoints: number }[],
): number | null {
  if (points.length === 0) return null;
  const totalBasisPoints = points.reduce(
    (sum, point) => sum + point.coverageBasisPoints,
    0,
  );
  return Math.round((totalBasisPoints / points.length / 100) * 10) / 10;
}

/** Renders one category's field selector and resolution-aware line chart. @param props - System, category, and shared query bounds. @returns The category chart card. */
export function CategoryChart({
  systemId,
  category,
  durationMillis,
  resolution,
  timezone,
  maximumPoints,
}: CategoryChartProps) {
  const { t, i18n } = useTranslation();
  const [field, setField] = useState<SeriesField>(category.fields[0]);
  const query = useSeries({
    systemId,
    durationMillis,
    field,
    resolution,
    timezone,
    maximumPoints,
  });
  const series = query.data?.series[0];
  const points = series?.points ?? [];

  let content: ReactNode = null;
  if (query.isPending) {
    content = <Skeleton className="h-64 w-full" />;
  } else if (query.isError) {
    content = (
      <Alert variant="destructive">
        <AlertTitle>{t("charts.loadErrorTitle")}</AlertTitle>
        <AlertDescription>{t("charts.loadErrorDescription")}</AlertDescription>
      </Alert>
    );
  } else if (points.length === 0) {
    content = (
      <p className="text-sm text-muted-foreground">{t("charts.noData")}</p>
    );
  } else if (series) {
    const data = points.map((point) => ({
      timestamp: point.timestampEpochMillis,
      value: convertSeriesValue(series.unit, point.value),
    }));
    content = (
      <div className="flex flex-col gap-2">
        <SeriesLineChart
          actualResolution={query.data.actualResolution}
          color={CATEGORY_COLORS[category.key]}
          data={data}
          locale={i18n.language}
          seriesLabel={t(`charts.field.${field}`)}
          timezone={query.data.timezone}
          unitSymbol={seriesUnitSymbol(series.unit)}
        />
        <p className="text-xs text-muted-foreground">
          {t("charts.resolutionSummary", {
            resolution: t(
              `charts.actualResolution.${query.data.actualResolution}`,
            ),
            coverage: averageCoveragePercent(points),
          })}
        </p>
      </div>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t(`charts.category.${category.key}`)}</CardTitle>
        {category.fields.length > 1 ? (
          <CardDescription>
            <ToggleGroup
              aria-label={t("charts.fieldLabel")}
              onValueChange={(value: string) => {
                if (value) setField(value as SeriesField);
              }}
              size="sm"
              type="single"
              value={field}
              variant="outline"
            >
              {category.fields.map((option) => (
                <ToggleGroupItem key={option} value={option}>
                  {t(`charts.field.${option}`)}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </CardDescription>
        ) : null}
      </CardHeader>
      <CardContent>{content}</CardContent>
    </Card>
  );
}
