import type {
  ResolutionParam,
  SeriesField,
} from "@/features/charts/api/chartsApi";
import {
  type ChartView,
  ChartActionsBar,
} from "@/features/charts/components/ChartActionsBar";
import { AnalysisExportStatus } from "@/features/charts/components/AnalysisExportStatus";
import { CategoryChartBody } from "@/features/charts/components/CategoryChartBody";
import { CategoryFieldSelector } from "@/features/charts/components/CategoryFieldSelector";
import { useAnalysisExport } from "@/features/charts/hooks/useAnalysisExport";
import { useCategorySeriesData } from "@/features/charts/hooks/useCategorySeriesData";
import type {
  ChartCategoryDefinition,
  ChartCategoryKey,
} from "@/features/charts/utils/chartCategories";
import { Card, CardContent, CardHeader, CardTitle } from "@/shared/components";
import { useReducedMotion } from "@/shared/hooks";
import { useState } from "react";
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

/** Renders one category's field selector, view/compare/export actions, and resolution-aware line chart or accessible table. @param props - System, category, and shared query bounds. @returns The category chart card. */
export function CategoryChart({
  systemId,
  category,
  durationMillis,
  resolution,
  timezone,
  maximumPoints,
}: CategoryChartProps) {
  const { t, i18n } = useTranslation();
  const reducedMotion = useReducedMotion();
  const [field, setField] = useState<SeriesField>(category.fields[0]);
  const [view, setView] = useState<ChartView>("chart");
  const [compareEnabled, setCompareEnabled] = useState(false);
  const { query, series, points, values, summary, previousSummary } =
    useCategorySeriesData({
      systemId,
      field,
      durationMillis,
      resolution,
      timezone,
      maximumPoints,
      compareEnabled,
    });
  const exportMutation = useAnalysisExport();

  const handleExport = (format: "csv" | "json") => {
    const endEpochMillis = Date.now();
    exportMutation.mutate({
      systemId,
      field,
      startEpochMillis: endEpochMillis - durationMillis,
      endEpochMillis,
      resolution,
      timezone,
      maximumPoints,
      format,
    });
  };

  const data = points.map((point, index) => ({
    timestamp: point.timestampEpochMillis,
    value: values[index] ?? 0,
  }));

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">
          {t(`charts.category.${category.key}`)}
        </CardTitle>
        <CategoryFieldSelector
          fields={category.fields}
          onChange={setField}
          value={field}
        />
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <ChartActionsBar
          compareEnabled={compareEnabled}
          exportPending={exportMutation.isPending}
          onCompareChange={setCompareEnabled}
          onExport={handleExport}
          onViewChange={setView}
          view={view}
        />
        <CategoryChartBody
          actualResolution={query.data?.actualResolution}
          color={CATEGORY_COLORS[category.key]}
          compareEnabled={compareEnabled}
          data={data}
          field={field}
          gaps={series?.gaps ?? []}
          isError={query.isError}
          isPending={query.isPending}
          locale={i18n.language}
          points={points}
          previousSummary={previousSummary}
          reducedMotion={reducedMotion}
          summary={summary}
          timezone={query.data?.timezone}
          unit={series?.unit}
          view={view}
        />
        <AnalysisExportStatus
          error={exportMutation.isError}
          queuedJobId={
            exportMutation.data?.kind === "queued"
              ? exportMutation.data.jobId
              : null
          }
        />
      </CardContent>
    </Card>
  );
}
