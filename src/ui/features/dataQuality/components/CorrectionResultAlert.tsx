import { Alert, AlertDescription, AlertTitle } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Correction result alert properties. */
export interface CorrectionResultAlertProps {
  /** Failure returned by the correction mutation, if any. */ error: Error | null;
  /** Whether the correction was accepted. */ success: boolean;
}

/** Renders the acceptance confirmation or the failure alert of a correction, distinguishing a lost optimistic-concurrency race. @param props - Mutation outcome. @returns The result alert, or nothing while idle. */
export function CorrectionResultAlert({
  error,
  success,
}: CorrectionResultAlertProps) {
  const { t } = useTranslation();
  if (success)
    return (
      <Alert>
        <AlertTitle>{t("dataQuality.correction.successTitle")}</AlertTitle>
        <AlertDescription>
          {t("dataQuality.correction.successDescription")}
        </AlertDescription>
      </Alert>
    );
  if (error === null) return null;
  return (
    <Alert variant="destructive">
      <AlertTitle>{t("dataQuality.correction.errorTitle")}</AlertTitle>
      <AlertDescription>
        {error.message === "correction_conflict"
          ? t("dataQuality.correction.conflictDescription")
          : t("dataQuality.correction.errorDescription")}
      </AlertDescription>
    </Alert>
  );
}
