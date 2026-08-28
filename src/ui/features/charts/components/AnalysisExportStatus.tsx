import { Alert, AlertDescription, AlertTitle } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Analysis export status properties. */
export interface AnalysisExportStatusProps {
  /** Whether the export request failed. */ error: boolean;
  /** Identifier of the queued export job, or `null` when nothing is queued. */ queuedJobId:
    string | null;
}

/** Renders the failure alert and queued-job hint for an analysis export. @param props - Export failure flag and queued job identifier. @returns The export status messages. */
export function AnalysisExportStatus({
  error,
  queuedJobId,
}: AnalysisExportStatusProps) {
  const { t } = useTranslation();
  return (
    <>
      {error ? (
        <Alert variant="destructive">
          <AlertTitle>{t("charts.export.errorTitle")}</AlertTitle>
          <AlertDescription>
            {t("charts.export.errorDescription")}
          </AlertDescription>
        </Alert>
      ) : null}
      {queuedJobId === null ? null : (
        <p className="text-xs text-muted-foreground">
          {t("charts.export.queuedDescription", { jobId: queuedJobId })}
        </p>
      )}
    </>
  );
}
