import { z } from "zod";

const optionalPositiveInteger = z.number().int().positive().optional();
const provenanceSchema = z.object({
  sourceName: z.string().min(1),
  sourceReference: z.url(),
  retrievedOn: z.string().nullable().optional(),
});
const temperatureRangeSchema = z.object({
  minimumMilliCelsius: z.number().int(),
  maximumMilliCelsius: z.number().int(),
});
const dimensionsSchema = z.object({
  length: z.number().int().positive(),
  width: z.number().int().positive(),
  height: z.number().int().positive(),
});

/** Validated inverter catalog entry returned by the offline catalog API. */
export const inverterCatalogEntrySchema = z.object({
  id: z.string().min(1),
  revision: z.string().min(1),
  manufacturer: z.string().min(1),
  model: z.string().min(1),
  provenance: provenanceSchema,
  dc: z.object({
    topology: z.string().nullable().optional(),
    totalStringInputCount: z.number().int().positive(),
    maximumInputVoltageMillivolts: optionalPositiveInteger,
    startVoltageMillivolts: optionalPositiveInteger,
    nominalInputVoltageMillivolts: optionalPositiveInteger,
    minimumMpptVoltageMillivolts: optionalPositiveInteger,
    maximumMpptVoltageMillivolts: optionalPositiveInteger,
    maximumInputCurrentMilliamperes: optionalPositiveInteger,
    maximumShortCircuitCurrentMilliamperes: optionalPositiveInteger,
    mpptInputs: z.array(
      z.object({
        trackerIndex: z.number().int().positive(),
        stringInputCount: z.number().int().positive(),
        maximumOperatingCurrentMilliamperes: optionalPositiveInteger,
        maximumShortCircuitCurrentMilliamperes: optionalPositiveInteger,
        minimumVoltageMillivolts: optionalPositiveInteger,
        maximumVoltageMillivolts: optionalPositiveInteger,
      }),
    ),
  }),
  ac: z
    .object({
      phaseCount: z.number().int().positive(),
      ratedActivePowerWatts: z.number().int().positive(),
    })
    .loose(),
  operational: z
    .object({
      maximumEfficiencyBasisPoints: optionalPositiveInteger,
      europeanEfficiencyBasisPoints: optionalPositiveInteger,
      communicationInterfaces: z.array(z.string()).default([]),
      dimensionsMillimetres: dimensionsSchema.optional(),
      weightGrams: optionalPositiveInteger,
      operatingTemperature: temperatureRangeSchema.optional(),
    })
    .loose(),
});

/** Validated solar-module catalog entry returned by the offline catalog API. */
export const solarModuleCatalogEntrySchema = z.object({
  id: z.string().min(1),
  revision: z.string().min(1),
  manufacturer: z.string().min(1),
  model: z.string().min(1),
  provenance: provenanceSchema,
  specification: z
    .object({
      cellTechnology: z.string(),
      cellDescription: z.string().nullable().optional(),
      bifacial: z.boolean(),
      bifacialityFactorBasisPoints: optionalPositiveInteger,
      bifacialityToleranceBasisPoints: optionalPositiveInteger,
      peakPowerWatts: z.number().int().positive(),
      openCircuitVoltageMillivolts: z.number().int().positive(),
      maximumPowerVoltageMillivolts: z.number().int().positive(),
      shortCircuitCurrentMilliamperes: z.number().int().positive(),
      maximumPowerCurrentMilliamperes: z.number().int().positive(),
      efficiencyBasisPoints: z.number().int().positive(),
      shortCircuitCurrentTemperatureCoefficientPpmPerCelsius: z.number().int(),
      openCircuitVoltageTemperatureCoefficientPpmPerCelsius: z.number().int(),
      peakPowerTemperatureCoefficientPpmPerCelsius: z.number().int(),
      maximumSystemVoltageMillivolts: z.number().int().positive(),
      operatingTemperature: temperatureRangeSchema,
      maximumSeriesFuseMilliamperes: z.number().int().positive(),
      maximumFrontStaticLoadPascals: z.number().int().positive(),
      maximumRearStaticLoadPascals: z.number().int().positive(),
      dimensionsMillimetres: dimensionsSchema,
      weightGrams: z.number().int().positive(),
    })
    .loose(),
});

/** @property search - Bounded free-text manufacturer/model query. @property manufacturer - Optional exact manufacturer filter. @property offset - Zero-based result offset. @property limit - Requested page size, clamped to 100. */
export interface EquipmentCatalogQuery {
  search?: string;
  manufacturer?: string;
  offset?: number;
  limit?: number;
}

/** Deterministic catalog response page with release revision metadata. */
export interface EquipmentCatalogPage<T> {
  revision: string;
  total: number;
  offset: number;
  limit: number;
  items: T[];
}

export type InverterCatalogEntry = z.infer<typeof inverterCatalogEntrySchema>;
export type SolarModuleCatalogEntry = z.infer<
  typeof solarModuleCatalogEntrySchema
>;
