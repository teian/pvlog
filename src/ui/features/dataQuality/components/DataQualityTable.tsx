import type {
  DataQualityIssue,
  DataQualityKind,
} from "@/features/dataQuality/api/dataQualityApi";
import { formatTimestamp } from "@/features/charts/utils/formatTimestamp";
import {
  Badge,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

const SEVERE_KINDS = new Set<DataQualityKind>([
  "missing_interval",
  "rejected_ingestion",
  "counter_reset",
]);

/** Data quality table properties. */
export interface DataQualityTableProps {
  /** Issues to list, ordered by start time. */ issues: DataQualityIssue[];
  /** IANA timezone used to format the listed interval boundaries. */
  timezone: string;
  /** Active i18next locale. */ locale: string;
}

/** Lists data-quality issues in an accessible table, using badge text (not color alone) to convey severity. @param props - Issues and formatting context. @returns The data-quality table. */
export function DataQualityTable({
  issues,
  timezone,
  locale,
}: DataQualityTableProps) {
  const { t } = useTranslation();
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>{t("dataQuality.table.kind")}</TableHead>
          <TableHead>{t("dataQuality.table.range")}</TableHead>
          <TableHead>{t("dataQuality.table.sources")}</TableHead>
          <TableHead>{t("dataQuality.table.reason")}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {issues.map((issue) => (
          <TableRow key={`${String(issue.startEpochMillis)}-${issue.kind}`}>
            <TableCell>
              <Badge
                variant={
                  SEVERE_KINDS.has(issue.kind) ? "destructive" : "secondary"
                }
              >
                {t(`dataQuality.kind.${issue.kind}`)}
              </Badge>
            </TableCell>
            <TableCell>
              {t("dataQuality.table.rangeValue", {
                start: formatTimestamp(
                  issue.startEpochMillis,
                  "hourly",
                  timezone,
                  locale,
                ),
                end: formatTimestamp(
                  issue.endEpochMillis,
                  "hourly",
                  timezone,
                  locale,
                ),
              })}
            </TableCell>
            <TableCell>
              {issue.sourceReferences.length > 0
                ? issue.sourceReferences.join(", ")
                : t("dataQuality.table.noSources")}
            </TableCell>
            <TableCell>
              {issue.reasonCode ?? t("dataQuality.table.noReason")}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
