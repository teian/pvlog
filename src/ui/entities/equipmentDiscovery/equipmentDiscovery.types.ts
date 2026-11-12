import { z } from "zod";

const id = z.uuid();

/** A device a data source reported that matches no inverter of the system. */
export const discoveryProposalSchema = z.object({
  id,
  deviceSerial: z.string(),
  reportedName: z.string().nullable().default(null),
  model: z.string().nullable().default(null),
  firmware: z.string().nullable().default(null),
  state: z.enum(["open", "dismissed"]),
  trackers: z.array(z.number().int().min(1).max(255)),
  channelCount: z.number().int().nonnegative(),
  firstSeenAtEpochMillis: z.number().int(),
  lastSeenAtEpochMillis: z.number().int(),
});

/** The equipment an adoption created. */
export const adoptedEquipmentSchema = z.object({
  inverterId: id,
  strings: z.array(
    z.object({
      stringId: id,
      tracker: z.number().int().nullable().default(null),
      name: z.string(),
    }),
  ),
  boundChannels: z.number().int().nonnegative(),
});

export type DiscoveryProposal = z.infer<typeof discoveryProposalSchema>;
export type AdoptedEquipment = z.infer<typeof adoptedEquipmentSchema>;

/**
 * How a proposed device is taken over as equipment.
 *
 * @property name - Name of the inverter.
 * @property strings - Strings to create; `tracker` is the DC input whose measurement channels the string receives, or null for a string without one.
 */
export interface DiscoveryAdoption {
  name: string;
  strings: {
    tracker: number | null;
    name: string;
    capacityWatts: number;
  }[];
}
