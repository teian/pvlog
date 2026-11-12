import type { HistoryImport } from "@/entities/dataSource";
import { useTranslation } from "react-i18next";

/** How many rejected rows the report lists; the server keeps up to 500. */
const LISTED_REJECTIONS = 20;

/** Shows what a dry run found in a file and, once committed, what was stored. @param props - The dry run or import. @returns The report as a description list. */
export function HistoryImportReportView({
  historyImport,
}: {
  historyImport: HistoryImport;
}) {
  const { t } = useTranslation();
  const { report, result } = historyImport;
  const facts: [string, string][] = [
    [
      t("dataSources.import.report.kind"),
      t(`dataSources.import.kinds.${historyImport.kind}`),
    ],
    [t("dataSources.import.report.rows"), String(report.rows)],
    [t("dataSources.import.report.accepted"), String(report.accepted)],
    [t("dataSources.import.report.rejected"), String(report.rejectedCount)],
  ];
  if (report.firstDate !== null && report.lastDate !== null)
    facts.push([
      t("dataSources.import.report.period"),
      t("dataSources.import.report.periodValue", {
        first: report.firstDate,
        last: report.lastDate,
      }),
    ]);
  facts.push(
    [
      t("dataSources.import.report.alreadyImported"),
      String(report.overlap.alreadyImported),
    ],
    [
      t("dataSources.import.report.otherSources"),
      String(report.overlap.otherSources),
    ],
  );
  if (result)
    facts.push(
      [t("dataSources.import.result.stored"), String(result.stored)],
      [
        t("dataSources.import.result.alreadyPresent"),
        String(result.alreadyPresent),
      ],
      [t("dataSources.import.result.skipped"), String(result.skipped)],
    );
  return (
    <div className="flex flex-col gap-3 text-sm">
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        {facts.map(([term, value]) => (
          <div className="contents" key={term}>
            <dt className="text-muted-foreground">{term}</dt>
            <dd className="tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      {report.rejected.length > 0 ? (
        <details>
          <summary>
            {t("dataSources.import.rejectedRows", {
              count: report.rejectedCount,
            })}
          </summary>
          <ul className="mt-2 flex flex-col gap-1">
            {report.rejected.slice(0, LISTED_REJECTIONS).map((rejection) => (
              <li key={rejection.row}>
                {t("dataSources.import.rejectedRow", {
                  row: rejection.row,
                  reason: t(`dataSources.import.reasons.${rejection.reason}`, {
                    defaultValue: rejection.reason,
                  }),
                  column: rejection.column ?? "–",
                })}
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  );
}
