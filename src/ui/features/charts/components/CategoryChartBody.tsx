import type {
  SeriesField,
  SeriesPoint,
  SeriesQueryResult,
  SeriesUnit,
} from "@/features/charts/api/chartsApi";
import type { ChartView } from "@/features/charts/components/ChartActionsBar";
import { SeriesLineChart } from "@/features/charts/components/SeriesLineChart";
import { SeriesTable } from "@/features/charts/components/SeriesTable";
import {
  seriesUnitSymbol,
  type SeriesSummary,
} from "@/features/charts/utils/formatSeriesValue";
import { Alert, AlertDescription, AlertTitle, Skeleton } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Category chart body properties. */
export interface CategoryChartBodyProps {
  /** Whether the primary query is still loading. */ isPending: boolean;
  /** Whether the primary query failed. */ isError: boolean;
  /** Points already converted to display units, paired with their timestamp. */
  data: { timestamp: number; value: number }[];
  /** Raw points, used for coverage and the accessible table. */
  points: SeriesPoint[];
  /** Canonical unit of the primary series, once loaded. */ unit: SeriesUnit | undefined;
  /** Gaps for the primary series. */ gaps: SeriesQueryResult["series"][number]["gaps"];
  /** Resolution/timezone metadata from the primary query response. */
  actualResolution: string | undefined;
  /** IANA timezone reported by the primary query response. */
  timezone: string | undefined;
  /** Active i18next locale. */ locale: string;
  /** Selected field, used for the series label. */ field: SeriesField;
  /** Summary statistics for the primary series. */ summary: SeriesSummary | null;
  /** Whether the previous-period comparison is shown. */
  compareEnabled: boolean;
  /** Summary statistics for the previous period, if loaded. */
  previousSummary: SeriesSummary | null;
  /** Chart or table view. */ view: ChartView;
  /** Line color, typically a `--chart-N` design token reference. */ color: string;
  /** Disables chart animation for `prefers-reduced-motion`. */
  reducedMotion: boolean;
}

/** Renders the loading, error, empty, or loaded (chart/table + summary) states for one category's series. @param props - Query state, converted data, and display context. @returns The category chart body. */
export function CategoryChartBody({
  isPending,
  isError,
  data,
  points,
  unit,
  gaps,
  actualResolution,
  timezone,
  locale,
  field,
  summary,
  compareEnabled,
  previousSummary,
  view,
  color,
  reducedMotion,
}: CategoryChartBodyProps) {
  const { t } = useTranslation();
  if (isPending) return <Skeleton className="h-64 w-full" />;
  if (isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("charts.loadErrorTitle")}</AlertTitle>
        <AlertDescription>{t("charts.loadErrorDescription")}</AlertDescription>
      </Alert>
    );
  if (points.length === 0 || !unit || !actualResolution || !timezone)
    return <p className="text-sm text-muted-foreground">{t("charts.noData")}</p>;

  const unitSymbol = seriesUnitSymbol(unit);
  const numberOptions = { maximumFractionDigits: 1 };
  return (
    <div className="flex flex-col gap-3">
      {view === "table" ? (
        <SeriesTable
          actualResolution={actualResolution}
          gaps={gaps}
          locale={locale}
          points={points}
          timezone={timezone}
          unit={unit}
        />
      ) : (
        <SeriesLineChart
          actualResolution={actualResolution}
          color={color}
          data={data}
          gaps={gaps}
          locale={locale}
          reducedMotion={reducedMotion}
          seriesLabel={t(`charts.field.${field}`)}
          timezone={timezone}
          unitSymbol={unitSymbol}
        />
      )}
      {summary ? (
        <p className="text-xs text-muted-foreground">
          {t("charts.resolutionSummary", {
            resolution: t(`charts.actualResolution.${actualResolution}`),
            coverage:
              Math.round(
                (points.reduce((sum, p) => sum + p.coverageBasisPoints, 0) /
                  points.length /
                  100) *
                  10,
              ) / 10,
          })}{" "}
          {t("charts.summary", {
            count: summary.count,
            minimum: summary.minimum.toLocaleString(locale, numberOptions),
            maximum: summary.maximum.toLocaleString(locale, numberOptions),
            average: summary.average.toLocaleString(locale, numberOptions),
            unit: unitSymbol,
          })}
        </p>
      ) : null}
      {compareEnabled && summary && previousSummary && previousSummary.average !== 0 ? (
        <p className="text-xs text-muted-foreground">
          {t("charts.comparisonSummary", {
            average: previousSummary.average.toLocaleString(
              locale,
              numberOptions,
            ),
            unit: unitSymbol,
            delta: (
              ((summary.average - previousSummary.average) /
                Math.abs(previousSummary.average)) *
              100
            ).toLocaleString(locale, { ...numberOptions, signDisplay: "always" }),
          })}
        </p>
      ) : null}
    </div>
  );
}
