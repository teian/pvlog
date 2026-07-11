import { useTranslation } from "react-i18next";

/** A compact, accessible ranking list for comparison and ladder results. @param props - Rows and accessible list label. @returns The ranking list. */
export function RankingList({
  entries,
  label,
}: {
  entries: {
    rank: number;
    systemId: string;
    displayName: string;
    normalizedGenerationWhPerKw: number;
    coverageBasisPoints: number;
  }[];
  label: string;
}) {
  const { t } = useTranslation();
  return (
    <ol aria-label={label} className="space-y-2">
      {entries.map((entry) => (
        <li className="rounded-md border p-3" key={entry.systemId}>
          <p className="font-medium">
            {t("community.ladder.entry", {
              rank: entry.rank,
              name: entry.displayName,
            })}
          </p>
          <p className="text-sm text-muted-foreground">
            {t("community.ladder.metrics", {
              generation: entry.normalizedGenerationWhPerKw,
              coverage: entry.coverageBasisPoints / 100,
            })}
          </p>
        </li>
      ))}
    </ol>
  );
}
