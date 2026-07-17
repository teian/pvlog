import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import type { Dashboard } from "../api/dashboardApi";

/** Dashboard health panel properties. */
export interface DashboardHealthPanelsProps {
  /** Validated operational dashboard projection. */
  data: Dashboard;
}

/** Renders data coverage and ingestion health. @param props - Dashboard projection. @returns Operational health cards. */
export function DashboardHealthPanels({ data }: DashboardHealthPanelsProps) {
  const { t } = useTranslation();
  const coverage = data.coverageBasisPoints / 100;

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-semibold">
            {t("dashboard.coverageTitle")}
          </CardTitle>
          <CardDescription>
            {t("dashboard.coverageDescription", { value: coverage.toFixed(1) })}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div
            aria-label={t("dashboard.coverageLabel")}
            aria-valuemax={100}
            aria-valuemin={0}
            aria-valuenow={coverage}
            className="h-3 overflow-hidden rounded-full bg-muted"
            role="progressbar"
          >
            <div
              className="h-full bg-primary transition-[width]"
              style={{ width: `${String(coverage)}%` }}
            />
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm font-semibold">
            {t("dashboard.ingestionTitle")}
          </CardTitle>
          <CardDescription>
            {t("dashboard.ingestionDescription", {
              lag: data.ingestion.lagSeconds,
            })}
          </CardDescription>
        </CardHeader>
        <CardContent className="grid grid-cols-2 gap-4 text-sm">
          <p className="font-mono tabular-nums">
            {t("dashboard.accepted", { count: data.ingestion.acceptedToday })}
          </p>
          <p className="font-mono tabular-nums">
            {t("dashboard.rejected", { count: data.ingestion.rejectedToday })}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
