import {
  managedInverterSchema,
  type ManagedInverter,
  type SystemInverterDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import { timeToMinutes } from "@/features/systemManagement/utils/systemManagementDraft";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const collection = (systemId: string) =>
  `/api/v1/systems/${systemId}/inverters`;

/** Loads the inverters of a system with their strings. @param systemId - System whose equipment to load. @returns Validated inverters. */
export async function fetchInverters(
  systemId: string,
): Promise<ManagedInverter[]> {
  return z
    .array(managedInverterSchema)
    .parse(await sessionJsonRequest(collection(systemId)));
}

/**
 * Converts an inverter draft into the request the API expects.
 *
 * A string keeps its identifier, so measurement channels bound to it stay
 * bound; a string without one is created.
 */
function inverterInput(inverter: SystemInverterDraft) {
  return {
    name: inverter.name.trim(),
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
      ...(string.id ? { id: string.id } : {}),
      name: string.name.trim(),
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
      temperatureCoefficientPpmPerCelsius: Math.round(
        string.temperatureCoefficient * 10_000,
      ),
      shading: string.shading.map((entry) => ({
        fromMinuteOfDay: timeToMinutes(entry.from),
        toMinuteOfDay: timeToMinutes(entry.to),
        label: entry.label.trim(),
        lossBasisPoints: Math.round(entry.degree * 100),
      })),
      effectiveFrom: string.effectiveFrom,
      effectiveTo: string.effectiveTo,
    })),
  };
}

/** Saves a system's equipment: updates kept inverters, creates new ones and removes those left out. @param systemId - System whose equipment to save. @param drafts - Complete equipment as edited. @param current - Inverters as last loaded. @returns Completion after every change was stored. */
export async function saveInverters(
  systemId: string,
  drafts: SystemInverterDraft[],
  current: ManagedInverter[],
): Promise<void> {
  const retained = new Set(
    drafts.flatMap((inverter) => (inverter.id ? [inverter.id] : [])),
  );
  const removed = current.filter(({ id }) => !retained.has(id));
  await Promise.all([
    ...drafts.map((inverter) =>
      sessionJsonRequest(
        inverter.id
          ? `${collection(systemId)}/${inverter.id}`
          : collection(systemId),
        {
          method: inverter.id ? "PUT" : "POST",
          body: JSON.stringify(inverterInput(inverter)),
        },
      ),
    ),
    ...removed.map((inverter) =>
      sessionJsonRequest(`${collection(systemId)}/${inverter.id}`, {
        method: "DELETE",
      }),
    ),
  ]);
}
