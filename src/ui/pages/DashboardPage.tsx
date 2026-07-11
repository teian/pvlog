import { useDashboard } from "@/features/dashboard";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { DashboardSkeleton } from "@/pages/DashboardSkeleton";
import { useTranslation } from "react-i18next";

/** Displays current operations without presenting stale telemetry as live. @returns The operational dashboard. */
export function DashboardPage() {
  const { t } = useTranslation();
  const dashboard = useDashboard();
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
  const ageSeconds = data.ageSeconds;
  const fresh = ageSeconds <= data.freshnessThresholdSeconds;
  const metrics = [
    {
      key: "generation",
      value: `${String(Math.round(data.generationWatts))} W`,
    },
    {
      key: "consumption",
      value:
        data.consumptionWatts === null
          ? t("dashboard.unavailable")
          : `${String(Math.round(data.consumptionWatts))} W`,
    },
    {
      key: "grid",
      value:
        data.gridWatts === null
          ? t("dashboard.unavailable")
          : `${String(Math.round(data.gridWatts))} W`,
    },
    {
      key: "battery",
      value:
        data.batteryBasisPoints === null
          ? t("dashboard.unavailable")
          : `${(data.batteryBasisPoints / 100).toFixed(0)}%`,
    },
  ];
  return (
    <section className="flex flex-col gap-6">
      <header>
        <h1 className="text-2xl font-bold tracking-tight">
          {t("dashboard.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {fresh
            ? t("dashboard.live", { age: ageSeconds })
            : t("dashboard.stale", { age: ageSeconds })}
        </p>
      </header>
      {!fresh ? (
        <Alert variant="destructive">
          <AlertTitle>{t("dashboard.staleTitle")}</AlertTitle>
          <AlertDescription>{t("dashboard.staleDescription")}</AlertDescription>
        </Alert>
      ) : null}
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        {metrics.map((metric) => (
          <Card key={metric.key}>
            <CardHeader>
              <CardDescription>
                {t(`dashboard.kpi.${metric.key}`)}
              </CardDescription>
              <CardTitle className="text-2xl tabular-nums">
                {metric.value}
              </CardTitle>
            </CardHeader>
          </Card>
        ))}
      </div>
      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>{t("dashboard.coverageTitle")}</CardTitle>
            <CardDescription>
              {t("dashboard.coverageDescription", {
                value: (data.coverageBasisPoints / 100).toFixed(1),
              })}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div
              aria-label={t("dashboard.coverageLabel")}
              className="h-3 overflow-hidden rounded-full bg-muted"
            >
              <div
                className="h-full bg-primary"
                style={{ width: `${String(data.coverageBasisPoints / 100)}%` }}
              />
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>{t("dashboard.ingestionTitle")}</CardTitle>
            <CardDescription>
              {t("dashboard.ingestionDescription", {
                lag: data.ingestion.lagSeconds,
              })}
            </CardDescription>
          </CardHeader>
          <CardContent className="grid grid-cols-2 gap-4 text-sm">
            <p>
              {t("dashboard.accepted", { count: data.ingestion.acceptedToday })}
            </p>
            <p>
              {t("dashboard.rejected", { count: data.ingestion.rejectedToday })}
            </p>
          </CardContent>
        </Card>
      </div>
      <Card>
        <CardHeader>
          <CardTitle>{t("dashboard.alertsTitle")}</CardTitle>
          <CardDescription>{t("dashboard.alertsDescription")}</CardDescription>
        </CardHeader>
        <CardContent>
          <ul className="flex flex-col gap-3">
            {data.recentAlerts.length === 0 ? (
              <li className="text-sm text-muted-foreground">
                {t("dashboard.noAlerts")}
              </li>
            ) : (
              data.recentAlerts.map((alert) => (
                <li
                  className="flex items-center justify-between border-b border-border pb-3 text-sm"
                  key={alert.id}
                >
                  <span>{alert.title}</span>
                  <span className="font-mono text-xs text-muted-foreground">
                    {t(`dashboard.alertState.${alert.state}`)}
                  </span>
                </li>
              ))
            )}
          </ul>
        </CardContent>
      </Card>
    </section>
  );
}
