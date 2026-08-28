import { useSession } from "@/features/auth";
import type { DataQualityKind } from "@/features/dataQuality/api/dataQualityApi";
import { useDataQuality } from "@/features/dataQuality/hooks/useDataQuality";
import { RANGE_PRESETS, type RangePresetKey } from "@/shared/lib";
import { useCallback, useEffect, useState } from "react";

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

/** Owns range, kind filter, and reconciliation state for the data-quality view, and derives the filtered issue list plus the caller's correction permission. @param systemId - System whose telemetry quality is inspected. @returns Filter state, derived issues, permission flag, and the reconciliation trigger. */
export function useDataQualityView(systemId: string) {
  const session = useSession();
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
    session.data?.permissions.includes("telemetry_write") ?? false;
  const startReconciling = useCallback(() => {
    setReconciling(true);
  }, []);

  return {
    activeKinds,
    canCorrect,
    issues,
    query,
    rangeKey,
    reconciling,
    setActiveKinds,
    setRangeKey,
    startReconciling,
  };
}
