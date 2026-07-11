import {
  useLadder,
  useSystemComparison,
} from "@/features/community/hooks/useCommunity";
import { RankingList } from "@/features/community/components/RankingList";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders authorized comparisons and the public normalized-generation ladder. @param props - Systems available to the active session. @returns The comparison panel. */
export function CommunityComparisonPanel({
  systemIds,
}: {
  systemIds: string[];
}) {
  const { t } = useTranslation();
  const ladder = useLadder();
  const comparison = useSystemComparison();
  const canCompare = systemIds.length >= 2;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("community.comparison.title")}</CardTitle>
        <CardDescription>
          {t("community.comparison.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Button
          disabled={!canCompare || comparison.isPending}
          onClick={() => {
            comparison.mutate(systemIds.slice(0, 20));
          }}
          type="button"
        >
          {t("community.comparison.compare")}
        </Button>
        {!canCompare ? (
          <p className="text-sm text-muted-foreground">
            {t("community.comparison.requiresSystems")}
          </p>
        ) : null}
        {comparison.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("community.unavailable")}
          </p>
        ) : null}
        {comparison.data ? (
          <RankingList
            entries={comparison.data}
            label={t("community.comparison.results")}
          />
        ) : null}
        <h2 className="text-sm font-semibold uppercase tracking-widest text-muted-foreground">
          {t("community.ladder.title")}
        </h2>
        {ladder.isLoading ? <Skeleton className="h-20 w-full" /> : null}
        {ladder.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("community.unavailable")}
          </p>
        ) : null}
        {ladder.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("community.ladder.empty")}
          </p>
        ) : null}
        {ladder.data ? (
          <RankingList entries={ladder.data} label={t("community.ladder.title")} />
        ) : null}
      </CardContent>
    </Card>
  );
}
