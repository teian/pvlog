import { useSession } from "@/features/auth";
import {
  ForecastCompletenessPanel,
  ForecastSettingsForm,
  ForecastYieldPanel,
  PerformancePanel,
  useForecastCompleteness,
  useForecastSettings,
  usePerformanceSeries,
  useYieldSeries,
  type ForecastRange,
} from "@/features/forecasting";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";

const DAY = 86_400_000;

/** Renders forecast administration, forward yield, and historical performance for one system. @returns The system forecasting page. */
export function SystemForecastPage() {
  const { t } = useTranslation();
  const { systemId = "" } = useParams();
  const session = useSession();
  const accountId = session.data?.accountId ?? "";
  const [now] = useState(() => Date.now());
  const forwardRange: ForecastRange = {
    startEpochMillis: now,
    endEpochMillis: now + 3 * DAY,
    resolution: "hour",
    maximumPoints: 1000,
  };
  const historyRange: ForecastRange = {
    startEpochMillis: now - 30 * DAY,
    endEpochMillis: now,
    resolution: "day",
    maximumPoints: 1000,
  };
  const settings = useForecastSettings(accountId, systemId);
  const completeness = useForecastCompleteness(accountId, systemId);
  const forecast = useYieldSeries(
    accountId,
    systemId,
    forwardRange,
    "forecast",
  );
  const performance = usePerformanceSeries(
    accountId,
    systemId,
    historyRange,
    "generation_performance",
  );
  const realization = usePerformanceSeries(
    accountId,
    systemId,
    historyRange,
    "forecast_realization",
  );
  return (
    <section
      aria-labelledby="forecast-page-title"
      className="flex flex-col gap-6"
    >
      <header className="flex flex-col gap-1">
        <h1
          className="text-2xl font-extrabold tracking-tight"
          id="forecast-page-title"
        >
          {t("forecasting.page.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("forecasting.page.description")}
        </p>
      </header>
      <div className="grid gap-4 xl:grid-cols-2">
        <ForecastCompletenessPanel
          {...(completeness.data === undefined
            ? {}
            : { completeness: completeness.data })}
          error={completeness.isError}
          loading={completeness.isPending}
        />
        <ForecastSettingsForm
          accountId={accountId}
          error={settings.isError}
          loading={settings.isPending}
          systemId={systemId}
          {...(settings.data === undefined ? {} : { versioned: settings.data })}
        />
      </div>
      <ForecastYieldPanel
        endEpochMillis={forwardRange.endEpochMillis}
        error={forecast.isError}
        loading={forecast.isPending}
        {...(forecast.data === undefined ? {} : { series: forecast.data })}
        startEpochMillis={forwardRange.startEpochMillis}
        systemId={systemId}
      />
      <PerformancePanel
        error={performance.isError || realization.isError}
        loading={performance.isPending || realization.isPending}
        {...(performance.data === undefined
          ? {}
          : { performance: performance.data })}
        range={historyRange}
        {...(realization.data === undefined
          ? {}
          : { realization: realization.data })}
        systemId={systemId}
      />
    </section>
  );
}
