import {
  firstDataSchema,
  geocodingResultSchema,
  setupStatusSchema,
  systemRecordSchema,
  type CreateSystemInput,
  type FirstData,
  type GeocodingResult,
  type SetupStatus,
  type SystemRecord,
  type UpdateSystemInput,
} from "@/entities/system/system.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

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
    throw new Error(`location_search_failed:${String(response.status)}`);
  return z.array(geocodingResultSchema).parse(await response.json());
}

/** Loads one system. @param systemId - System to load. @returns The validated system record. */
export async function fetchSystem(systemId: string): Promise<SystemRecord> {
  return systemRecordSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}`),
  );
}

/** Creates a system; only the name is required. @param input - Name and optional setup fields. @returns The created system record. */
export async function createSystem(
  input: CreateSystemInput,
): Promise<SystemRecord> {
  return systemRecordSchema.parse(
    await sessionJsonRequest("/api/v1/systems", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  );
}

/** Replaces a system's editable fields when the given version is current. @param systemId - System to change. @param version - Version the edit is based on. @param input - Complete editable fields. @returns The updated system record. */
export async function updateSystem(
  systemId: string,
  version: number,
  input: UpdateSystemInput,
): Promise<SystemRecord> {
  return systemRecordSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}`, {
      method: "PUT",
      headers: { "if-match": `"${String(version)}"` },
      body: JSON.stringify(input),
    }),
  );
}

/** Loads what a system has and what it still needs. @param systemId - System to inspect. @returns The validated setup status. */
export async function fetchSetupStatus(systemId: string): Promise<SetupStatus> {
  return setupStatusSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}/setup-status`),
  );
}

/** Checks whether a system has received data. @param systemId - System to inspect. @returns Whether real data arrived, with the newest reading. */
export async function fetchFirstData(systemId: string): Promise<FirstData> {
  return firstDataSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}/first-data`),
  );
}

/** Archives or restores a system when the given version is current. @param systemId - System to change. @param version - Version the action is based on. @param action - Whether to archive or restore. @returns The updated system record. */
export async function setSystemArchived(
  systemId: string,
  version: number,
  action: "archive" | "restore",
): Promise<SystemRecord> {
  return systemRecordSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}/${action}`, {
      method: "POST",
      headers: { "if-match": `"${String(version)}"` },
    }),
  );
}

/** Permanently deletes a system with all its data. @param systemId - System to delete. @param version - Version the deletion is based on. @returns Completion after deletion. */
export async function deleteSystem(
  systemId: string,
  version: number,
): Promise<void> {
  await sessionJsonRequest(`/api/v1/systems/${systemId}`, {
    method: "DELETE",
    headers: {
      "if-match": `"${String(version)}"`,
      "x-confirm-delete": "true",
    },
  });
}
