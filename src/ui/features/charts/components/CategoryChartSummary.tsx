import type { SeriesPoint } from "@/features/charts/api/chartsApi";
import type { SeriesSummary } from "@/features/charts/utils/formatSeriesValue";
import { useTranslation } from "react-i18next";

const NUMBER_OPTIONS = { maximumFractionDigits: 1 };

/** Averages point coverage and rounds it to one decimal percent. */
function averageCoveragePercent(points: SeriesPoint[]): number {
  const total = points.reduce(
    (sum, point) => sum + point.coverageBasisPoints,
    0,
  );
  return Math.round((total / points.length / 100) * 10) / 10;
}

/** Category chart summary properties. */
export interface CategoryChartSummaryProps {
  /** Resolution reported by the query response, used for the label. */ actualResolution: string;
  /** Whether the previous-period comparison is shown. */ compareEnabled: boolean;
  /** Active i18next locale. */ locale: string;
  /** Raw points, used to derive coverage. */ points: SeriesPoint[];
  /** Summary statistics for the previous period, if loaded. */ previousSummary: SeriesSummary | null;
  /** Summary statistics for the primary series. */ summary: SeriesSummary | null;
  /** Display symbol of the series unit. */ unitSymbol: string;
}

/** Renders the resolution/coverage summary and the optional previous-period comparison for one category's series. @param props - Summary statistics and display context. @returns The summary lines, or nothing when no summary exists. */
export function CategoryChartSummary({
  actualResolution,
  compareEnabled,
  locale,
  points,
  previousSummary,
  summary,
  unitSymbol,
}: CategoryChartSummaryProps) {
  const { t } = useTranslation();
  if (!summary) return null;
  const comparable =
    compareEnabled && previousSummary !== null && previousSummary.average !== 0;
  return (
    <>
      <p className="text-xs text-muted-foreground">
        {t("charts.resolutionSummary", {
          resolution: t(`charts.actualResolution.${actualResolution}`),
          coverage: averageCoveragePercent(points),
        })}{" "}
        {t("charts.summary", {
          count: summary.count,
          minimum: summary.minimum.toLocaleString(locale, NUMBER_OPTIONS),
          maximum: summary.maximum.toLocaleString(locale, NUMBER_OPTIONS),
          average: summary.average.toLocaleString(locale, NUMBER_OPTIONS),
          unit: unitSymbol,
        })}
      </p>
      {comparable ? (
        <p className="text-xs text-muted-foreground">
          {t("charts.comparisonSummary", {
            average: previousSummary.average.toLocaleString(
              locale,
              NUMBER_OPTIONS,
            ),
            unit: unitSymbol,
            delta: (
              ((summary.average - previousSummary.average) /
                Math.abs(previousSummary.average)) *
              100
            ).toLocaleString(locale, {
              ...NUMBER_OPTIONS,
              signDisplay: "always",
            }),
          })}
        </p>
      ) : null}
    </>
  );
}
