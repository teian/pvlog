import type { VerifiedReading } from "@/entities/system";
import { useTranslation } from "react-i18next";

/** Shows a received reading in normalized form with where it came from. @param props - The reading to show. @returns A definition list of the reading's time, source and values. */
export function SetupReadingSummary({ reading }: { reading: VerifiedReading }) {
  const { t, i18n } = useTranslation();
  const number = new Intl.NumberFormat(i18n.language);
  const time = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "medium",
  });
  const values = [
    ["generationPower", reading.generationPowerWatts, "W"],
    ["generationEnergy", reading.generationEnergyWh, "Wh"],
    ["consumptionPower", reading.consumptionPowerWatts, "W"],
    ["consumptionEnergy", reading.consumptionEnergyWh, "Wh"],
  ] as const;
  return (
    <dl className="grid gap-x-6 gap-y-2 rounded-md border bg-muted/30 p-4 text-sm sm:grid-cols-2">
      <div>
        <dt className="text-muted-foreground">
          {t("setup.verify.measuredAt")}
        </dt>
        <dd>{time.format(reading.measuredAtEpochMillis)}</dd>
      </div>
      <div>
        <dt className="text-muted-foreground">{t("setup.verify.source")}</dt>
        <dd>{reading.dataSourceName ?? t("setup.verify.sourceUnknown")}</dd>
      </div>
      {values.map(([key, value, unit]) =>
        value === null ? null : (
          <div key={key}>
            <dt className="text-muted-foreground">
              {t(`setup.verify.${key}`)}
            </dt>
            <dd className="font-mono">
              {number.format(value)} {unit}
            </dd>
          </div>
        ),
      )}
    </dl>
  );
}
