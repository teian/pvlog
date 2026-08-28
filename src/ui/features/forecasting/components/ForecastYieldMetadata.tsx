import { ForecastMetadata } from "@/features/forecasting/components/ForecastMetadata";
import type { YieldSeries } from "@/features/forecasting/types/forecast.types";
import { useTranslation } from "react-i18next";

/** Forward forecast metadata properties. */
export interface ForecastYieldMetadataProps {
  /** Total forecast energy in kilowatt-hours. */ energy: number;
  /** Active i18next locale. */ locale: string;
  /** Peak forecast power in kilowatts. */ peak: number;
  /** Forward modeled result being described. */ series: YieldSeries;
}

/** Summarizes energy, peak, issue time, horizon, provenance, model, capacity, and configuration of a forward forecast. @param props - Derived totals, locale, and the series. @returns The metadata definition list. */
export function ForecastYieldMetadata({
  energy,
  locale,
  peak,
  series,
}: ForecastYieldMetadataProps) {
  const { t } = useTranslation();
  const date = (value: number | null) =>
    value === null
      ? t("forecasting.notAvailable")
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(value);
  const first = series.points[0]?.intervalStart ?? null;
  const last = series.points.at(-1)?.intervalEnd ?? null;
  return (
    <dl className="grid gap-3 text-sm sm:grid-cols-2 lg:grid-cols-4">
      <ForecastMetadata
        label={t("forecasting.forward.energy")}
        value={t("forecasting.energyKwh", {
          value: energy.toLocaleString(locale, { maximumFractionDigits: 1 }),
        })}
      />
      <ForecastMetadata
        label={t("forecasting.forward.peak")}
        value={t("forecasting.powerKw", {
          value: peak.toLocaleString(locale, { maximumFractionDigits: 1 }),
        })}
      />
      <ForecastMetadata
        label={t("forecasting.forward.issued")}
        value={date(series.issueTime)}
      />
      <ForecastMetadata
        label={t("forecasting.forward.horizon")}
        value={`${date(first)} – ${date(last)}`}
      />
      <ForecastMetadata
        label={t("forecasting.forward.provider")}
        value={series.provenance.attribution}
      />
      <ForecastMetadata
        label={t("forecasting.forward.model")}
        value={`${series.modelIdentifier} v${String(series.modelRevision)}`}
      />
      <ForecastMetadata
        label={t("forecasting.forward.capacity")}
        value={t("forecasting.capacityKwp", {
          included: (series.includedCapacityWatts / 1000).toLocaleString(
            locale,
          ),
          total: (series.totalEffectiveCapacityWatts / 1000).toLocaleString(
            locale,
          ),
        })}
      />
      <ForecastMetadata
        label={t("forecasting.forward.configuration")}
        value={series.configurationDigest.slice(0, 12)}
      />
    </dl>
  );
}
