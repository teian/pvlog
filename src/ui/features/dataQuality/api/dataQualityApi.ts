import { z } from "zod";

/** Kind of data-quality issue reported for a system. */
export const dataQualityKindSchema = z.enum([
  "missing_interval",
  "suspect_observation",
  "source_conflict",
  "counter_reset",
  "rejected_ingestion",
  "aggregate_lag",
]);

/** Kind of data-quality issue reported for a system. */
export type DataQualityKind = z.infer<typeof dataQualityKindSchema>;

const dataQualityIssueSchema = z.object({
  kind: dataQualityKindSchema,
  startEpochMillis: z.number().int(),
  endEpochMillis: z.number().int(),
  sourceReferences: z.array(z.string()),
  reasonCode: z.string().nullable().optional(),
});

/** A single reported data-quality issue. */
export type DataQualityIssue = z.infer<typeof dataQualityIssueSchema>;

/** Retrieves missing/suspect/conflicting/rejected data-quality issues for a bounded range. @param systemId - System to inspect. @param startEpochMillis - Inclusive UTC range start in epoch milliseconds. @param endEpochMillis - Exclusive UTC range end in epoch milliseconds. @param signal - Cancels the request when a newer query supersedes it. @returns The ordered data-quality issues. */
export async function fetchDataQuality(
  systemId: string,
  startEpochMillis: number,
  endEpochMillis: number,
  signal?: AbortSignal,
): Promise<DataQualityIssue[]> {
  const query = new URLSearchParams({
    startEpochMillis: String(startEpochMillis),
    endEpochMillis: String(endEpochMillis),
  });
  const response = await fetch(
    `/api/v1/systems/${systemId}/data-quality?${query.toString()}`,
    { credentials: "same-origin", signal: signal ?? null },
  );
  if (!response.ok)
    throw new Error(`data_quality_failed:${String(response.status)}`);
  return z.array(dataQualityIssueSchema).parse(await response.json());
}

const versionedObservationSchema = z.object({
  id: z.string(),
  systemId: z.string(),
  values: z.unknown().nullable(),
  version: z.number().int(),
  archived: z.boolean(),
});

/** The merged observation state returned immediately after a correction or deletion. */
export type VersionedObservation = z.infer<typeof versionedObservationSchema>;

/** Parameters accepted by {@link correctObservation}. */
export interface CorrectObservationParams {
  /** System that owns the observation. */ systemId: string;
  /** Observation to correct or delete. */ observationId: string;
  /** Version the client last observed; the server rejects stale writes. */
  expectedVersion: number;
  /** Audited justification for the change. */ reason: string;
  /** Replacement generation power reading, in watts; the only correctable field the API exposes today. */
  generationPowerWatts?: number;
  /** Deletes the observation instead of replacing a value. */ delete: boolean;
}

/** Applies an optimistic correction or deletion overlay to one observation and schedules reconciliation. @param params - Target observation, expected version, reason, and either a replacement value or deletion. @returns The immediately visible merged observation. */
export async function correctObservation(
  params: CorrectObservationParams,
): Promise<VersionedObservation> {
  const response = await fetch(
    `/api/v1/systems/${params.systemId}/observations/${params.observationId}`,
    {
      method: "PATCH",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        expectedVersion: params.expectedVersion,
        reason: params.reason,
        delete: params.delete,
        ...(params.generationPowerWatts === undefined
          ? {}
          : { generationPowerWatts: params.generationPowerWatts }),
      }),
    },
  );
  if (response.status === 409) throw new Error("correction_conflict");
  if (!response.ok)
    throw new Error(`correction_failed:${String(response.status)}`);
  return versionedObservationSchema.parse(await response.json());
}
