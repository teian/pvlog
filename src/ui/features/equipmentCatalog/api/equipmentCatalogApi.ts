import {
  inverterCatalogEntrySchema,
  solarModuleCatalogEntrySchema,
  type EquipmentCatalogPage,
  type EquipmentCatalogQuery,
  type InverterCatalogEntry,
  type SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const MAX_SEARCH_LENGTH = 80;
const pageFields = {
  revision: z.string().min(1),
  total: z.number().int().nonnegative(),
  offset: z.number().int().nonnegative(),
  limit: z.number().int().min(1).max(100),
};

function queryString(query: EquipmentCatalogQuery): string {
  const parameters = new URLSearchParams();
  const search = query.search?.trim().slice(0, MAX_SEARCH_LENGTH);
  const manufacturer = query.manufacturer?.trim().slice(0, MAX_SEARCH_LENGTH);
  if (search) parameters.set("search", search);
  if (manufacturer) parameters.set("manufacturer", manufacturer);
  parameters.set("offset", String(Math.max(0, query.offset ?? 0)));
  parameters.set(
    "limit",
    String(Math.min(100, Math.max(1, query.limit ?? 25))),
  );
  return parameters.toString();
}

async function getJson(path: string): Promise<unknown> {
  const response = await fetch(path, { credentials: "same-origin" });
  if (!response.ok)
    throw new Error(
      `equipment_catalog_request_failed:${String(response.status)}`,
    );
  return response.json();
}

/** Fetches a validated inverter catalog page. @param query - Bounded search and pagination values. @returns The validated deterministic page. */
export async function fetchInverterCatalog(
  query: EquipmentCatalogQuery,
): Promise<EquipmentCatalogPage<InverterCatalogEntry>> {
  return z
    .object({ ...pageFields, items: z.array(inverterCatalogEntrySchema) })
    .parse(
      await getJson(
        `/api/v1/equipment-catalog/inverters?${queryString(query)}`,
      ),
    );
}

/** Fetches a validated solar-module catalog page. @param query - Bounded search and pagination values. @returns The validated deterministic page. */
export async function fetchSolarModuleCatalog(
  query: EquipmentCatalogQuery,
): Promise<EquipmentCatalogPage<SolarModuleCatalogEntry>> {
  return z
    .object({ ...pageFields, items: z.array(solarModuleCatalogEntrySchema) })
    .parse(
      await getJson(
        `/api/v1/equipment-catalog/solar-modules?${queryString(query)}`,
      ),
    );
}

/** Persists confirmed editable equipment through the real aggregate API. @param systemId - Owning system. @param input - Confirmed aggregate. @returns The stored server response. */
export async function saveEquipmentConfiguration(
  systemId: string,
  input: unknown,
): Promise<unknown> {
  return sessionJsonRequest(`/api/v1/systems/${systemId}/inverters`, {
    method: "POST",
    body: JSON.stringify(input),
  });
}
