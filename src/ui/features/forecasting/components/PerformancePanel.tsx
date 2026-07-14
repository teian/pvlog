import { PerformanceMetricCard } from "@/features/forecasting/components/PerformanceMetricCard";
import type {
  ForecastRange,
  PerformanceSeries,
} from "@/features/forecasting/types/forecast.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Performance panel properties. */
export interface PerformancePanelProps {
  /** Active system. */ systemId: string;
  /** Expected-generation comparison. */ performance?: PerformanceSeries;
  /** Issued-forecast comparison. */ realization?: PerformanceSeries;
  /** Shared bounded range. */ range: ForecastRange;
  /** Whether either query is loading. */ loading: boolean;
  /** Whether either query failed. */ error: boolean;
}

/** Separately presents generation performance and forecast realization without labeling either as inverter efficiency. @param props - Query state and aligned modeled/measured series. @returns Two explicit comparison views. */
export function PerformancePanel({
  systemId,
  performance,
  realization,
  range,
  loading,
  error,
}: PerformancePanelProps) {
  const { t } = useTranslation();
  if (loading) return <Skeleton className="h-96 w-full" />;
  if (error)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("forecasting.performance.errorTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.performance.errorDescription")}
        </AlertDescription>
      </Alert>
    );
  return (
    <section
      aria-labelledby="performance-heading"
      className="flex flex-col gap-4"
    >
      <div>
        <h2 className="text-xl font-semibold" id="performance-heading">
          {t("forecasting.performance.title")}
        </h2>
        <p className="text-sm text-muted-foreground">
          {t("forecasting.performance.description")}
        </p>
      </div>
      <div className="grid gap-4 xl:grid-cols-2">
        <PerformanceMetricCard
          range={range}
          series={performance}
          systemId={systemId}
          type="generation_performance"
        />
        <PerformanceMetricCard
          range={range}
          series={realization}
          systemId={systemId}
          type="forecast_realization"
        />
      </div>
    </section>
  );
}
