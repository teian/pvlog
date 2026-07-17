import { z } from "zod";

const id = z.uuid();

/** Persisted PV string returned by the nested inverter API. */
export const managedStringSchema = z.object({
  id,
  inverterId: id,
  name: z.string(),
  panelCount: z.number().int().positive(),
  panelManufacturer: z.string().nullable(),
  panelModel: z.string().nullable(),
  ratedPowerWatts: z.number().int().positive(),
  moduleSpecificationSnapshot: z.unknown().nullable(),
  modulePeakPowerWatts: z.number().int().nullable(),
  totalPeakPowerWatts: z.number().int().nullable(),
  orientationDegrees: z.number().int().nullable(),
  tiltDegrees: z.number().int().nullable(),
  effectiveFrom: z.number().int(),
  effectiveTo: z.number().int().nullable(),
});

/** Persisted inverter aggregate returned by the nested inverter API. */
export const managedInverterSchema = z.object({
  id,
  systemId: id,
  name: z.string(),
  manufacturer: z.string().nullable(),
  model: z.string().nullable(),
  ratedPowerWatts: z.number().int().nullable(),
  specificationSnapshot: z.unknown().nullable(),
  effectiveFrom: z.number().int(),
  effectiveTo: z.number().int().nullable(),
  version: z.number().int().positive(),
  strings: z.array(managedStringSchema),
});

/** Mutable system lifecycle representation used by management actions. */
export const managedSystemRecordSchema = z.object({
  id,
  accountId: id,
  name: z.string(),
  timezone: z.string(),
  visibility: z.enum(["private", "account", "unlisted", "public"]),
  lifecycle: z.enum(["active", "archived", "pending_deletion"]),
  version: z.number().int().positive(),
  createdAt: z.number().int(),
  updatedAt: z.number().int(),
});

/** Complete system-management card model. */
export interface ManagedSystem {
  record: z.infer<typeof managedSystemRecordSchema>;
  inverters: z.infer<typeof managedInverterSchema>[];
}

/** Editable string values held locally until the wizard is submitted. */
export interface SystemStringDraft {
  id?: string;
  effectiveFrom: number;
  effectiveTo: number | null;
  name: string;
  panelCount: number;
  panelManufacturer: string;
  panelModel: string;
  modulePeakPowerWatts: number;
  orientationDegrees: number;
  tiltDegrees: number;
  temperatureCoefficient: number;
  shading: {
    id: string;
    from: string;
    to: string;
    label: string;
    degree: number;
  }[];
  moduleSpecificationSnapshot: unknown;
}

/** Editable inverter aggregate held locally until submission. */
export interface SystemInverterDraft {
  id?: string;
  effectiveFrom: number;
  effectiveTo: number | null;
  name: string;
  manufacturer: string;
  model: string;
  ratedPowerWatts: number;
  specificationSnapshot: unknown;
  strings: SystemStringDraft[];
}

/** Complete single-page wizard draft. */
export interface SystemWizardDraft {
  name: string;
  location: string;
  timezone: string;
  active: boolean;
  inverters: SystemInverterDraft[];
}

export type ManagedSystemRecord = z.infer<typeof managedSystemRecordSchema>;
export type ManagedInverter = z.infer<typeof managedInverterSchema>;
export type ManagedString = z.infer<typeof managedStringSchema>;
