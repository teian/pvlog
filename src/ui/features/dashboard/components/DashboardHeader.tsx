import type { DataState } from "@/entities/system";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
} from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { TriangleAlertIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Dashboard header properties. */
export interface DashboardHeaderProps {
  /** Age of the latest observation in seconds. */
  ageSeconds: number;
  /** Title of the first active alert, when present. */
  alertTitle?: string | undefined;
  /** Localized current date. */
  date: string;
  /** Whether readings exist and are current; only current data is presented as live. */
  dataState: DataState;
  /** Number of systems in the current account scope. */
  systemCount: number;
}

/** Renders dashboard identity, freshness, and the active fault callout. @param props - Current dashboard state. @returns The dashboard header region. */
export function DashboardHeader({
  ageSeconds,
  alertTitle,
  date,
  dataState,
  systemCount,
}: DashboardHeaderProps) {
  const { t } = useTranslation();
  const fresh = dataState === "current";

  return (
    <>
      <header className="flex flex-col gap-1">
        <div className="flex flex-wrap items-center gap-2">
          <h1 className="text-2xl font-extrabold tracking-tight">
            {t("dashboard.title")}
          </h1>
          {alertTitle ? (
            <Badge variant="fault">{t("dashboard.fault")}</Badge>
          ) : null}
        </div>
        <p className="text-xs text-muted-foreground">
          {t("dashboard.systemSummary", { count: systemCount, date })}
        </p>
      </header>
      {dataState === "none" ? null : (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <span
            aria-hidden="true"
            className={cn(
              "size-2 rounded-full",
              fresh ? "bg-success motion-safe:animate-pulse" : "bg-destructive",
            )}
          />
          {fresh
            ? t("dashboard.live", { age: ageSeconds })
            : t("dashboard.stale", { age: ageSeconds })}
        </p>
      )}
      {fresh && alertTitle ? (
        <Alert variant="destructive">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>{alertTitle}</AlertTitle>
          <AlertDescription>
            {t("dashboard.activeAlertDescription")}
          </AlertDescription>
        </Alert>
      ) : null}
    </>
  );
}
