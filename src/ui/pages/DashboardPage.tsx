import { useSession } from "@/features/auth";
import {
  DashboardHeader,
  DashboardHealthPanels,
  DashboardMetricGrid,
  DashboardRecentAlerts,
  useDashboard,
  type DashboardMetric,
} from "@/features/dashboard";
import { DashboardSkeleton } from "@/pages/DashboardSkeleton";
import { Alert, AlertDescription, AlertTitle } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Displays current operations without presenting stale telemetry as live. @returns The operational dashboard. */
export function DashboardPage() {
  const { i18n, t } = useTranslation();
  const dashboard = useDashboard();
  const session = useSession();

  if (dashboard.isPending) return <DashboardSkeleton />;
  if (dashboard.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("dashboard.loadErrorTitle")}</AlertTitle>
        <AlertDescription>
          {t("dashboard.loadErrorDescription")}
        </AlertDescription>
      </Alert>
    );

  const data = dashboard.data;
  const fresh = data.ageSeconds <= data.freshnessThresholdSeconds;
  const activeAlert = data.recentAlerts.find((alert) => alert.state === "open");
  const metrics: DashboardMetric[] = [
    {
      key: "generation",
      label: t("dashboard.kpi.generation"),
      value: `${String(Math.round(data.generationWatts))} W`,
      valueClassName: "text-primary",
    },
    {
      key: "consumption",
      label: t("dashboard.kpi.consumption"),
      value:
        data.consumptionWatts === null
          ? t("dashboard.unavailable")
          : `${String(Math.round(data.consumptionWatts))} W`,
      valueClassName: "text-brand",
    },
    {
      key: "grid",
      label: t("dashboard.kpi.grid"),
      value:
        data.gridWatts === null
          ? t("dashboard.unavailable")
          : `${String(Math.round(data.gridWatts))} W`,
      valueClassName: "text-success",
    },
    {
      key: "battery",
      label: t("dashboard.kpi.battery"),
      value:
        data.batteryBasisPoints === null
          ? t("dashboard.unavailable")
          : `${(data.batteryBasisPoints / 100).toFixed(0)}%`,
      valueClassName: "text-warning",
    },
  ];

  return (
    <section className="flex flex-col gap-6">
      <DashboardHeader
        ageSeconds={data.ageSeconds}
        alertTitle={activeAlert?.title}
        date={new Intl.DateTimeFormat(i18n.language, {
          day: "numeric",
          month: "long",
          weekday: "long",
        }).format(new Date())}
        fresh={fresh}
        systemCount={session.data?.systemIds.length ?? 0}
      />
      {!fresh ? (
        <Alert variant="destructive">
          <AlertTitle>{t("dashboard.staleTitle")}</AlertTitle>
          <AlertDescription>{t("dashboard.staleDescription")}</AlertDescription>
        </Alert>
      ) : null}
      <DashboardMetricGrid metrics={metrics} />
      <DashboardHealthPanels data={data} />
      <DashboardRecentAlerts alerts={data.recentAlerts} />
    </section>
  );
}
