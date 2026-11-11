import { z } from "zod";

const id = z.uuid();

/** What a measurement channel can be bound to. */
export const channelTargetKindSchema = z.enum([
  "unassigned",
  "plant",
  "inverter",
  "string",
]);

/** One named value series a data source delivers, with its binding. */
export const measurementChannelSchema = z.object({
  id,
  channelKey: z.string(),
  displayName: z.string(),
  unit: z.string(),
  targetKind: channelTargetKindSchema,
  targetId: id.nullable().default(null),
  tracksPower: z.boolean(),
  tracksEnergy: z.boolean(),
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
  tracksPower: boolean;
  tracksEnergy: boolean;
  displayName: string;
}
