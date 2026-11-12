import {
  adoptedEquipmentSchema,
  discoveryProposalSchema,
  type AdoptedEquipment,
  type DiscoveryAdoption,
  type DiscoveryProposal,
} from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

const collection = (systemId: string) =>
  `/api/v1/systems/${systemId}/discovery-proposals`;

/** Lists the open and the dismissed discovery proposals of a system. @param systemId - System whose proposals to list. @returns Validated proposals, oldest first. */
export async function fetchDiscoveryProposals(
  systemId: string,
): Promise<DiscoveryProposal[]> {
  return z
    .array(discoveryProposalSchema)
    .parse(
      await sessionJsonRequest(`${collection(systemId)}?includeDismissed=true`),
    );
}

/** Creates the inverter and strings of a proposal and assigns the device's measurement channels. @param systemId - Owning system. @param proposalId - Proposal to adopt. @param adoption - Inverter name and the strings to create. @returns The created equipment. */
export async function adoptDiscoveryProposal(
  systemId: string,
  proposalId: string,
  adoption: DiscoveryAdoption,
): Promise<AdoptedEquipment> {
  return adoptedEquipmentSchema.parse(
    await sessionJsonRequest(`${collection(systemId)}/${proposalId}/adopt`, {
      method: "POST",
      body: JSON.stringify(adoption),
    }),
  );
}

/** Dismisses a proposal so its device is not proposed again. @param systemId - Owning system. @param proposalId - Proposal to dismiss. @returns Completion after dismissing. */
export async function dismissDiscoveryProposal(
  systemId: string,
  proposalId: string,
): Promise<void> {
  await sessionJsonRequest(`${collection(systemId)}/${proposalId}/dismiss`, {
    method: "POST",
  });
}

/** Opens a dismissed proposal again. @param systemId - Owning system. @param proposalId - Proposal to open again. @returns Completion after the reset. */
export async function resetDiscoveryProposal(
  systemId: string,
  proposalId: string,
): Promise<void> {
  await sessionJsonRequest(`${collection(systemId)}/${proposalId}/reset`, {
    method: "POST",
  });
}
