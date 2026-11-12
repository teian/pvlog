import { z } from "zod";

const id = z.uuid();

/** What a measurement channel can be bound to. */
export const channelTargetKindSchema = z.enum([
  "unassigned",
  "plant",
  "inverter",
  "string",
]);

/** How a period of channel values is reduced to one value. */
export const channelAggregationSchema = z.enum([
  "mean",
  "sum",
  "min",
  "max",
  "last",
  "counter_delta",
]);

/** One named value series a data source delivers, with its definition and binding. `lastValue` is an integer with `scale` decimal places. */
export const measurementChannelSchema = z.object({
  id,
  channelKey: z.string(),
  displayName: z.string(),
  dataType: z.enum(["integer", "decimal", "boolean", "counter"]),
  unit: z.string(),
  scale: z.number().int().min(0).max(9),
  aggregation: channelAggregationSchema,
  targetKind: channelTargetKindSchema,
  targetId: id.nullable().default(null),
  discoveredAtEpochMillis: z.number().int().nullable().default(null),
  lastValueAtEpochMillis: z.number().int().nullable().default(null),
  lastValue: z.number().nullable().default(null),
});

/** Equipment a channel can be bound to, reduced to what the choice needs. */
export const bindableInverterSchema = z.object({
  id,
  name: z.string(),
  strings: z.array(z.object({ id, name: z.string() })),
});

export type ChannelTargetKind = z.infer<typeof channelTargetKindSchema>;
export type MeasurementChannel = z.infer<typeof measurementChannelSchema>;
export type BindableInverter = z.infer<typeof bindableInverterSchema>;

/**
 * Binding to store for a channel.
 *
 * @property targetId - Inverter or string identifier; null for the system or no binding.
 */
export interface ChannelBinding {
  targetKind: ChannelTargetKind;
  targetId: string | null;
  displayName: string;
}
