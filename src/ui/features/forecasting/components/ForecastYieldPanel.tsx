import { ForecastYieldCard } from "@/features/forecasting/components/ForecastYieldCard";
import type { YieldSeries } from "@/features/forecasting/types/forecast.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Forward forecast panel properties. */
export interface ForecastYieldPanelProps {
  /** Active system. */ systemId: string;
  /** Forward modeled result. */ series?: YieldSeries;
  /** Query loading state. */ loading: boolean;
  /** Query failure state. */ error: boolean;
  /** Export range start. */ startEpochMillis: number;
  /** Export range end. */ endEpochMillis: number;
}

/** Renders a responsive forward forecast with uncertainty, metadata, summaries, table alternative, and matching exports. @param props - Query state, system, and range. @returns Forward forecast experience. */
export function ForecastYieldPanel({
  systemId,
  series,
  loading,
  error,
  startEpochMillis,
  endEpochMillis,
}: ForecastYieldPanelProps) {
  const { t } = useTranslation();
  if (loading) return <Skeleton className="h-96 w-full" />;
  if (error)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("forecasting.forward.errorTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.forward.errorDescription")}
        </AlertDescription>
      </Alert>
    );
  if (!series || series.points.length === 0)
    return (
      <Alert>
        <AlertTitle>{t("forecasting.forward.emptyTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.forward.emptyDescription")}
        </AlertDescription>
      </Alert>
    );
  return (
    <ForecastYieldCard
      endEpochMillis={endEpochMillis}
      series={series}
      startEpochMillis={startEpochMillis}
      systemId={systemId}
    />
  );
}
