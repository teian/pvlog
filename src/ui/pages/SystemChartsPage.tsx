import { HistoricalCharts } from "@/features/charts";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";

/** Displays bounded historical charts for one system. @returns The system charts tab. */
export function SystemChartsPage() {
  const { t } = useTranslation();
  const { systemId } = useParams<{ systemId: string }>();

  return (
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-1">
        <h1 className="text-2xl font-extrabold tracking-tight">
          {t("charts.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("charts.description")}
        </p>
      </header>
      {systemId ? <HistoricalCharts systemId={systemId} /> : null}
    </div>
  );
}
