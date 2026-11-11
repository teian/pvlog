import {
  tariffSchema,
  type Tariff,
  type TariffInput,
} from "@/features/tariffs/tariffs.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const collection = (systemId: string) => `/api/v1/systems/${systemId}/tariffs`;

/** Lists the tariffs of a system. @param systemId - System whose tariffs to list. @returns Validated tariffs. */
export async function fetchTariffs(systemId: string): Promise<Tariff[]> {
  return z
    .array(tariffSchema)
    .parse(await sessionJsonRequest(collection(systemId)));
}

/** Creates a tariff, or replaces one when an identifier is given. @param systemId - Owning system. @param input - Tariff values. @param id - Tariff to replace; omitted to create one. @returns The stored tariff. */
export async function saveTariff(
  systemId: string,
  input: TariffInput,
  id?: string,
): Promise<Tariff> {
  return tariffSchema.parse(
    await sessionJsonRequest(
      id ? `${collection(systemId)}/${id}` : collection(systemId),
      { method: id ? "PUT" : "POST", body: JSON.stringify(input) },
    ),
  );
}

/** Deletes a tariff. @param systemId - Owning system. @param id - Tariff to delete. @returns Completion after deletion. */
export async function deleteTariff(
  systemId: string,
  id: string,
): Promise<void> {
  await sessionJsonRequest(`${collection(systemId)}/${id}`, {
    method: "DELETE",
  });
}
