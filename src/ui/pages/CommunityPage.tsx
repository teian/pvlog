import {
  CommunityComparisonPanel,
  CommunitySearchPanel,
  FavouritesPanel,
} from "@/features/community";
import { useSession } from "@/features/auth";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";

/** Displays discovery, favourites, comparisons, and ladder workflows. @returns The community page. */
export function CommunityPage() {
  const { t } = useTranslation();
  const session = useSession();
  return (
    <AppShell
      accountId={session.data?.accountId}
      systemIds={session.data?.systemIds}
    >
      <section aria-labelledby="community-title" className="space-y-2">
        <h1 className="text-2xl font-semibold" id="community-title">
          {t("community.title")}
        </h1>
        <p className="text-muted-foreground">{t("community.description")}</p>
      </section>
      <CommunitySearchPanel />
      <FavouritesPanel />
      <CommunityComparisonPanel systemIds={session.data?.systemIds ?? []} />
    </AppShell>
  );
}
