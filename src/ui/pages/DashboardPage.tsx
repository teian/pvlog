import { NoDataNotice, NoSystemNotice } from "@/entities/system";
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
  const header = (
    <DashboardHeader
      ageSeconds={data.ageSeconds}
      alertTitle={
        data.recentAlerts.find((alert) => alert.state === "open")?.title
      }
      dataState={data.dataState}
      date={new Intl.DateTimeFormat(i18n.language, {
        day: "numeric",
        month: "long",
        weekday: "long",
      }).format(new Date())}
      systemCount={data.systemCount}
    />
  );
  // Without a reading there is nothing to measure: zeros and a live marker
  // would read as "the system produces nothing right now".
  if (data.dataState === "none") {
    const systemIds = session.data?.systemIds ?? [];
    return (
      <section className="flex flex-col gap-6">
        {header}
        {data.systemCount === 0 ? (
          <NoSystemNotice
            canManage={
              session.data?.permissions.includes("system_manage") ?? false
            }
          />
        ) : (
          <NoDataNotice
            systemId={systemIds.length === 1 ? systemIds[0] : undefined}
          />
        )}
      </section>
    );
  }
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
      {header}
      {data.dataState === "stale" ? (
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
