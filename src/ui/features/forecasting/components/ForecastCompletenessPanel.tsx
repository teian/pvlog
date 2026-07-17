import type { ForecastInputCompleteness } from "@/features/forecasting/types/forecast.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";

/** Completeness panel properties. */
export interface ForecastCompletenessPanelProps {
  /** Effective readiness response, when loaded. */ completeness?: ForecastInputCompleteness;
  /** Whether the resource is loading. */ loading: boolean;
  /** Whether the resource failed. */ error: boolean;
}

/** Shows actionable forecast-input gaps and effective capacity. @param props - Query state and completeness response. @returns Localized readiness panel. */
export function ForecastCompletenessPanel({
  completeness,
  loading,
  error,
}: ForecastCompletenessPanelProps) {
  const { t } = useTranslation();
  if (loading) return <Card className="h-44 animate-pulse bg-muted" />;
  if (error || !completeness)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("forecasting.completeness.errorTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.completeness.errorDescription")}
        </AlertDescription>
      </Alert>
    );
  const coverage =
    completeness.totalEffectiveCapacityWatts > 0
      ? completeness.includedCapacityWatts /
        completeness.totalEffectiveCapacityWatts
      : null;
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm font-semibold">
          {t("forecasting.completeness.title")}
        </CardTitle>
        <CardDescription>
          {t("forecasting.completeness.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <Badge variant={completeness.complete ? "default" : "outline"}>
            {completeness.complete
              ? t("forecasting.completeness.ready")
              : t("forecasting.completeness.actionRequired")}
          </Badge>
          <span className="text-sm tabular-nums">
            {coverage === null
              ? t("forecasting.notAvailable")
              : t("forecasting.completeness.capacity", {
                  included: (
                    completeness.includedCapacityWatts / 1000
                  ).toLocaleString(),
                  total: (
                    completeness.totalEffectiveCapacityWatts / 1000
                  ).toLocaleString(),
                  percent: (coverage * 100).toLocaleString(undefined, {
                    maximumFractionDigits: 1,
                  }),
                })}
          </span>
        </div>
        {completeness.reasons.length > 0 ? (
          <ul className="flex list-disc flex-col gap-1 pl-5 text-sm">
            {completeness.reasons.map((reason) => (
              <li key={reason}>{t(`forecasting.reasons.${reason}`)}</li>
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("forecasting.completeness.noGaps")}
          </p>
        )}
        <Link
          className="w-fit text-sm font-medium underline underline-offset-4"
          to="/administration"
        >
          {t("forecasting.completeness.configureEquipment")}
        </Link>
      </CardContent>
    </Card>
  );
}
