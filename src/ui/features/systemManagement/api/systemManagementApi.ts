import {
  managedInverterSchema,
  managedSystemRecordSchema,
  type ManagedSystem,
  type SystemInverterDraft,
  type SystemWizardDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const geocodingResultSchema = z.object({
  displayName: z.string().min(1),
  latitude: z.number().min(-90).max(90),
  longitude: z.number().min(-180).max(180),
  attribution: z.string().min(1),
});

export type GeocodingResult = z.infer<typeof geocodingResultSchema>;

/** Searches address suggestions through the authenticated server-side provider. @param query - Partial free-form address. @param language - Preferred result language. @param limit - Maximum suggestions. @param signal - Optional cancellation signal. @returns Validated OpenStreetMap matches. */
export async function searchAddresses(
  query: string,
  language: string,
  limit = 5,
  signal?: AbortSignal,
): Promise<GeocodingResult[]> {
  const parameters = new URLSearchParams({
    q: query,
    language,
    limit: String(limit),
  });
  const response = await fetch(
    `/api/v1/geocoding/search?${parameters.toString()}`,
    {
      credentials: "same-origin",
      ...(signal ? { signal } : {}),
    },
  );
  if (!response.ok)
    throw new Error(
      `system_management_request_failed:${String(response.status)}`,
    );
  return z.array(geocodingResultSchema).parse(await response.json());
}

/** Loads mutable system metadata and its effective inverter/string tree. @param systemId - System to load. @returns Complete management card model. */
export async function fetchManagedSystem(
  systemId: string,
): Promise<ManagedSystem> {
  const [record, inverters] = await Promise.all([
    sessionJsonRequest(`/api/v1/systems/${systemId}`),
    sessionJsonRequest(`/api/v1/systems/${systemId}/inverters`),
  ]);
  return {
    record: managedSystemRecordSchema.parse(record),
    inverters: z.array(managedInverterSchema).parse(inverters),
  };
}

function inverterInput(inverter: SystemInverterDraft) {
  return {
    name: inverter.name,
    manufacturer: inverter.manufacturer || null,
    model: inverter.model || null,
    serialReference: null,
    ratedPowerWatts: inverter.ratedPowerWatts || null,
    valueProvenance: inverter.specificationSnapshot
      ? "catalog_copied"
      : "manual",
    specificationSnapshot: inverter.specificationSnapshot,
    effectiveFrom: inverter.effectiveFrom,
    effectiveTo: inverter.effectiveTo,
    strings: inverter.strings.map((string) => ({
      name: string.name,
      panelCount: string.panelCount,
      panelManufacturer: string.panelManufacturer || null,
      panelModel: string.panelModel || null,
      valueProvenance: string.moduleSpecificationSnapshot
        ? "catalog_copied"
        : "manual",
      moduleSpecificationSnapshot: string.moduleSpecificationSnapshot,
      modulePeakPowerWatts: string.modulePeakPowerWatts,
      orientationDegrees: string.orientationDegrees,
      tiltDegrees: string.tiltDegrees,
      effectiveFrom: string.effectiveFrom,
      effectiveTo: string.effectiveTo,
    })),
  };
}

async function saveInverters(
  systemId: string,
  inverters: SystemInverterDraft[],
  current: ManagedSystem | undefined,
) {
  const retained = new Set(
    inverters.flatMap((inverter) => (inverter.id ? [inverter.id] : [])),
  );
  const removed =
    current?.inverters.filter(({ id }) => !retained.has(id)) ?? [];
  await Promise.all([
    ...inverters.map((inverter) =>
      sessionJsonRequest(
        inverter.id
          ? `/api/v1/systems/${systemId}/inverters/${inverter.id}`
          : `/api/v1/systems/${systemId}/inverters`,
        {
          method: inverter.id ? "PUT" : "POST",
          body: JSON.stringify(inverterInput(inverter)),
        },
      ),
    ),
    ...removed.map((inverter) =>
      sessionJsonRequest(
        `/api/v1/systems/${systemId}/inverters/${inverter.id}`,
        {
          method: "DELETE",
        },
      ),
    ),
  ]);
}

/** Creates or updates a system and persists its complete editable inverter aggregates. @param draft - Validated wizard values. @param current - Existing system in edit mode. @returns Saved system record. */
export async function saveManagedSystem(
  draft: SystemWizardDraft,
  current?: ManagedSystem,
) {
  const body = current
    ? {
        name: draft.name,
        timezone: draft.timezone,
        visibility: current.record.visibility,
      }
    : { name: draft.name, timezone: draft.timezone };
  const value = await sessionJsonRequest(
    current ? `/api/v1/systems/${current.record.id}` : "/api/v1/systems",
    {
      method: current ? "PUT" : "POST",
      ...(current
        ? { headers: { "if-match": `"${String(current.record.version)}"` } }
        : {}),
      body: JSON.stringify(body),
    },
  );
  let record = managedSystemRecordSchema.parse(value);
  if (draft.active !== (record.lifecycle === "active")) {
    record = managedSystemRecordSchema.parse(
      await sessionJsonRequest(
        `/api/v1/systems/${record.id}/${draft.active ? "restore" : "archive"}`,
        {
          method: "POST",
          headers: { "if-match": `"${String(record.version)}"` },
        },
      ),
    );
  }
  await saveInverters(record.id, draft.inverters, current);
  return record;
}

/** Permanently deletes a confirmed system using optimistic concurrency. @param system - System to delete. @returns Completion after deletion. */
export async function deleteManagedSystem(
  system: ManagedSystem,
): Promise<void> {
  await sessionJsonRequest(`/api/v1/systems/${system.record.id}`, {
    method: "DELETE",
    headers: {
      "if-match": `"${String(system.record.version)}"`,
      "x-confirm-delete": "true",
    },
  });
}
