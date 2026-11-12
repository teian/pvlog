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
  temperatureCoefficientPpmPerCelsius: z
    .number()
    .int()
    .nullable()
    .default(null),
  shading: z
    .array(
      z.object({
        fromMinuteOfDay: z.number().int(),
        toMinuteOfDay: z.number().int(),
        label: z.string().default(""),
        lossBasisPoints: z.number().int(),
      }),
    )
    .default([]),
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
  serialReference: z.string().nullable().default(null),
  firmwareVersion: z.string().nullable().default(null),
  firmwareChangedAt: z.number().int().nullable().default(null),
  specificationSnapshot: z.unknown().nullable(),
  effectiveFrom: z.number().int(),
  effectiveTo: z.number().int().nullable(),
  version: z.number().int().positive(),
  strings: z.array(managedStringSchema),
});

/** Editable string values held locally until the equipment is saved. */
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

/** Editable inverter with its strings, held locally until the equipment is saved. */
export interface SystemInverterDraft {
  id?: string;
  effectiveFrom: number;
  effectiveTo: number | null;
  name: string;
  manufacturer: string;
  model: string;
  ratedPowerWatts: number;
  specificationSnapshot: unknown;
  /** Serial the device reports under; kept as stored, so the inverter stays linked to its data. */
  serialReference?: string | null;
  /** Firmware the device last reported; read-only. */
  firmwareVersion?: string | null;
  /** Epoch milliseconds of the last firmware change; read-only. */
  firmwareChangedAt?: number | null;
  strings: SystemStringDraft[];
}

export type ManagedInverter = z.infer<typeof managedInverterSchema>;
export type ManagedString = z.infer<typeof managedStringSchema>;
