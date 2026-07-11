import { useTranslation } from "react-i18next";
import { useSession } from "@/features/auth";
import { AppShell } from "@/widgets";

/**
 * Displays the initial application placeholder while vertical slices are added.
 *
 * @returns The accessible initial page.
 */
export function HomePage() {
  const { t } = useTranslation();
  const session = useSession();

  return (
    <AppShell
      accountId={session.data?.accountId}
      systemIds={session.data?.systemIds}
    >
      <h1 className="text-2xl font-bold tracking-tight">{t("home.title")}</h1>
      <p className="text-sm text-muted-foreground">{t("home.description")}</p>
    </AppShell>
  );
}
