import {
  bindableInverterSchema,
  measurementChannelSchema,
  type BindableInverter,
  type ChannelBinding,
  type MeasurementChannel,
} from "@/features/measurementChannels/measurementChannels.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const collection = (systemId: string) => `/api/v1/systems/${systemId}/channels`;

/** Lists the measurement channels of a system. @param systemId - System whose channels to list. @returns Validated channels with their bindings. */
export async function fetchChannels(
  systemId: string,
): Promise<MeasurementChannel[]> {
  return z
    .array(measurementChannelSchema)
    .parse(await sessionJsonRequest(collection(systemId)));
}

/** Lists the inverters and strings a channel can be bound to. @param systemId - System whose equipment to list. @returns Inverters with their strings. */
export async function fetchBindableEquipment(
  systemId: string,
): Promise<BindableInverter[]> {
  return z
    .array(bindableInverterSchema)
    .parse(await sessionJsonRequest(`/api/v1/systems/${systemId}/inverters`));
}

/** Stores the binding and name of a channel. @param systemId - Owning system. @param channelKey - Key of the channel as the source sends it. @param binding - Binding to store. @returns The updated channel. */
export async function bindChannel(
  systemId: string,
  channelKey: string,
  binding: ChannelBinding,
): Promise<MeasurementChannel> {
  return measurementChannelSchema.parse(
    await sessionJsonRequest(
      `${collection(systemId)}/${encodeURIComponent(channelKey)}`,
      { method: "PUT", body: JSON.stringify(binding) },
    ),
  );
}

/** Retires a channel so it is no longer offered or aggregated. @param systemId - Owning system. @param channelKey - Key of the channel to retire. @returns Completion after retiring. */
export async function retireChannel(
  systemId: string,
  channelKey: string,
): Promise<void> {
  await sessionJsonRequest(
    `${collection(systemId)}/${encodeURIComponent(channelKey)}`,
    { method: "DELETE" },
  );
}
