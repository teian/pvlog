import { useSession } from "@/features/auth";
import { HistoricalCharts } from "@/features/charts";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";

/** Displays bounded historical charts for one system. @returns The system charts page. */
export function SystemChartsPage() {
  const { t } = useTranslation();
  const session = useSession();
  const { systemId } = useParams<{ systemId: string }>();

  return (
    <AppShell
      accountId={session.data?.accountId}
      systemIds={session.data?.systemIds}
    >
      <section className="flex flex-col gap-6">
        <header>
          <h1 className="text-2xl font-bold tracking-tight">
            {t("charts.title")}
          </h1>
        </header>
        {systemId ? <HistoricalCharts systemId={systemId} /> : null}
      </section>
    </AppShell>
  );
}
