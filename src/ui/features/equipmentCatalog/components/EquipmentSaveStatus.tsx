import { useTranslation } from "react-i18next";

/** Equipment save status properties. */
export interface EquipmentSaveStatusProps {
  /** Whether the save was rejected. */ error: boolean;
  /** Whether the configuration was persisted. */ success: boolean;
}

/** Announces the outcome of persisting an equipment configuration. @param props - Save outcome. @returns The status message, or nothing while idle. */
export function EquipmentSaveStatus({
  error,
  success,
}: EquipmentSaveStatusProps) {
  const { t } = useTranslation();
  if (success)
    return (
      <p aria-live="polite" className="text-sm">
        {t("features.equipmentCatalog.saved")}
      </p>
    );
  if (!error) return null;
  return (
    <p className="text-sm text-destructive" role="alert">
      {t("features.equipmentCatalog.validationError")}
    </p>
  );
}
