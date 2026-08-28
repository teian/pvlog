import { useTranslation } from "react-i18next";

/** Forecast settings save status properties. */
export interface ForecastSettingsStatusProps {
  /** Whether the save was rejected. */ error: boolean;
  /** Whether the settings were persisted. */ success: boolean;
}

/** Announces the outcome of saving forecast settings next to the submit button. @param props - Save outcome. @returns The status message, or nothing while idle. */
export function ForecastSettingsStatus({
  error,
  success,
}: ForecastSettingsStatusProps) {
  const { t } = useTranslation();
  if (success)
    return <span className="text-sm">{t("forecasting.settings.saved")}</span>;
  if (!error) return null;
  return (
    <span className="text-sm text-destructive" role="alert">
      {t("forecasting.settings.saveError")}
    </span>
  );
}
