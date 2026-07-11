import type { SeriesGap, SeriesPoint, SeriesUnit } from "@/features/charts/api/chartsApi";
import { convertSeriesValue } from "@/features/charts/utils/formatSeriesValue";
import { formatTimestamp } from "@/features/charts/utils/formatTimestamp";
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

/** Accessible series table properties. */
export interface SeriesTableProps {
  /** Points to list, ordered by timestamp. */ points: SeriesPoint[];
  /** Missing/suspect/incomplete-coverage intervals for this field. */
  gaps: SeriesGap[];
  /** Canonical unit reported by the series. */ unit: SeriesUnit;
  /** Resolution actually returned by the series query. */
  actualResolution: string;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Active i18next locale. */ locale: string;
}

/** Renders a keyboard- and screen-reader-navigable table alternative to the line chart, including gap intervals as text. @param props - Points, gaps, and formatting context. @returns The accessible data table. */
export function SeriesTable({
  points,
  gaps,
  unit,
  actualResolution,
  timezone,
  locale,
}: SeriesTableProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-3">
      <Table>
        <TableCaption>{t("charts.table.caption")}</TableCaption>
        <TableHeader>
          <TableRow>
            <TableHead>{t("charts.table.timestamp")}</TableHead>
            <TableHead>{t("charts.table.value")}</TableHead>
            <TableHead>{t("charts.table.coverage")}</TableHead>
            <TableHead>{t("charts.table.corrected")}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {points.map((point) => (
            <TableRow key={point.timestampEpochMillis}>
              <TableCell>
                {formatTimestamp(
                  point.timestampEpochMillis,
                  actualResolution,
                  timezone,
                  locale,
                )}
              </TableCell>
              <TableCell className="tabular-nums">
                {convertSeriesValue(unit, point.value).toLocaleString(locale)}
              </TableCell>
              <TableCell className="tabular-nums">
                {t("charts.table.coveragePercent", {
                  value: (point.coverageBasisPoints / 100).toLocaleString(
                    locale,
                  ),
                })}
              </TableCell>
              <TableCell>
                {point.provenance
                  ? t("charts.table.correctedYes")
                  : t("charts.table.correctedNo")}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {gaps.length > 0 ? (
        <div>
          <p className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
            {t("charts.table.gapsHeading", { count: gaps.length })}
          </p>
          <ul className="mt-1 flex flex-col gap-1 text-sm">
            {gaps.map((gap) => (
              <li key={`${String(gap.startEpochMillis)}-${gap.kind}`}>
                {t("charts.table.gapRange", {
                  kind: t(`charts.gapKind.${gap.kind}`),
                  start: formatTimestamp(
                    gap.startEpochMillis,
                    actualResolution,
                    timezone,
                    locale,
                  ),
                  end: formatTimestamp(
                    gap.endEpochMillis,
                    actualResolution,
                    timezone,
                    locale,
                  ),
                })}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
