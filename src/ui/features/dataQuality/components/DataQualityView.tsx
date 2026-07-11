import { useSession } from "@/features/auth";
import type { DataQualityKind } from "@/features/dataQuality/api/dataQualityApi";
import { CorrectionForm } from "@/features/dataQuality/components/CorrectionForm";
import { DataQualityFilters } from "@/features/dataQuality/components/DataQualityFilters";
import { DataQualityTable } from "@/features/dataQuality/components/DataQualityTable";
import { useDataQuality } from "@/features/dataQuality/hooks/useDataQuality";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { RANGE_PRESETS, type RangePresetKey } from "@/shared/lib";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

const ALL_KINDS: DataQualityKind[] = [
  "missing_interval",
  "suspect_observation",
  "source_conflict",
  "counter_reset",
  "rejected_ingestion",
  "aggregate_lag",
];
const RECONCILIATION_POLL_MILLIS = 3000;
const RECONCILIATION_WINDOW_MILLIS = 15_000;

/** Data quality view properties. */
export interface DataQualityViewProps {
  /** System whose telemetry quality is inspected. */ systemId: string;
}

/** Renders data-quality inspection (filtered issue table) plus a correction/deletion tool with a reconciliation-progress indicator. @param props - Target system. @returns The data-quality view. */
export function DataQualityView({ systemId }: DataQualityViewProps) {
  const { t, i18n } = useTranslation();
  const session = useSession();
  const timezone = useMemo(
    () => Intl.DateTimeFormat().resolvedOptions().timeZone,
    [],
  );
  const [rangeKey, setRangeKey] = useState<RangePresetKey>("week");
  const [activeKinds, setActiveKinds] = useState<DataQualityKind[]>(ALL_KINDS);
  const [reconciling, setReconciling] = useState(false);
  const preset =
    RANGE_PRESETS.find((candidate) => candidate.key === rangeKey) ??
    RANGE_PRESETS[0];
  const query = useDataQuality(
    systemId,
    preset.durationMillis,
    reconciling ? RECONCILIATION_POLL_MILLIS : undefined,
  );

  useEffect(() => {
    if (!reconciling) return;
    const timeout = setTimeout(() => {
      setReconciling(false);
    }, RECONCILIATION_WINDOW_MILLIS);
    return () => {
      clearTimeout(timeout);
    };
  }, [reconciling]);

  const issues = (query.data ?? []).filter((issue) =>
    activeKinds.includes(issue.kind),
  );
  const canCorrect =
    session.data?.permissions.includes("telemetry:write") ?? false;

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
        <CorrectionForm
          onSubmitted={() => {
            setReconciling(true);
          }}
          systemId={systemId}
        />
      ) : null}
    </div>
  );
}
