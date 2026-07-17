import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import type { Dashboard } from "../api/dashboardApi";

/** Recent alert list properties. */
export interface DashboardRecentAlertsProps {
  /** Recent account-wide alert transitions. */
  alerts: Dashboard["recentAlerts"];
}

/** Renders the recent alert state list. @param props - Recent alert transitions. @returns The alert history card. */
export function DashboardRecentAlerts({ alerts }: DashboardRecentAlertsProps) {
  const { t } = useTranslation();

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">
          {t("dashboard.alertsTitle")}
        </CardTitle>
        <CardDescription>{t("dashboard.alertsDescription")}</CardDescription>
      </CardHeader>
      <CardContent>
        <ul className="flex flex-col gap-3">
          {alerts.length === 0 ? (
            <li className="text-sm text-muted-foreground">
              {t("dashboard.noAlerts")}
            </li>
          ) : (
            alerts.map((alert) => (
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
  );
}
