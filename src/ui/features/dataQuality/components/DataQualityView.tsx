import { CorrectionForm } from "@/features/dataQuality/components/CorrectionForm";
import { DataQualityFilters } from "@/features/dataQuality/components/DataQualityFilters";
import { DataQualityTable } from "@/features/dataQuality/components/DataQualityTable";
import { useDataQualityView } from "@/features/dataQuality/hooks/useDataQualityView";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";

/** Data quality view properties. */
export interface DataQualityViewProps {
  /** System whose telemetry quality is inspected. */ systemId: string;
}

/** Renders data-quality inspection (filtered issue table) plus a correction/deletion tool with a reconciliation-progress indicator. @param props - Target system. @returns The data-quality view. */
export function DataQualityView({ systemId }: DataQualityViewProps) {
  const { t, i18n } = useTranslation();
  const timezone = useMemo(
    () => Intl.DateTimeFormat().resolvedOptions().timeZone,
    [],
  );
  const {
    activeKinds,
    canCorrect,
    issues,
    query,
    rangeKey,
    reconciling,
    setActiveKinds,
    setRangeKey,
    startReconciling,
  } = useDataQualityView(systemId);

  return (
    <div className="flex flex-col gap-6">
      <DataQualityFilters
        activeKinds={activeKinds}
        onKindsChange={setActiveKinds}
        onRangeChange={setRangeKey}
        rangeKey={rangeKey}
      />
      {reconciling ? (
        <Alert>
          <AlertTitle>{t("dataQuality.reconcilingTitle")}</AlertTitle>
          <AlertDescription>
            {t("dataQuality.reconcilingDescription")}
          </AlertDescription>
        </Alert>
      ) : null}
      {query.isPending ? <Skeleton className="h-64 w-full" /> : null}
      {query.isError ? (
        <Alert variant="destructive">
          <AlertTitle>{t("dataQuality.loadErrorTitle")}</AlertTitle>
          <AlertDescription>
            {t("dataQuality.loadErrorDescription")}
          </AlertDescription>
        </Alert>
      ) : null}
      {query.isSuccess && issues.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t("dataQuality.noIssues")}
        </p>
      ) : null}
      {query.isSuccess && issues.length > 0 ? (
        <DataQualityTable
          issues={issues}
          locale={i18n.language}
          timezone={timezone}
        />
      ) : null}
      {canCorrect ? (
        <CorrectionForm onSubmitted={startReconciling} systemId={systemId} />
      ) : null}
    </div>
  );
}
