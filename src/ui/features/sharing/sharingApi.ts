import {
  sharingStateSchema,
  type SharingInput,
  type SharingState,
} from "@/features/sharing/sharing.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";

/** Loads the sharing state of a system. @param systemId - System to inspect. @returns The validated sharing state. */
export async function fetchSharing(systemId: string): Promise<SharingState> {
  return sharingStateSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}/sharing`),
  );
}

/** Changes the sharing of a system. @param systemId - System to change. @param input - Link state, optional rotation and visibility. @returns The updated sharing state. */
export async function updateSharing(
  systemId: string,
  input: SharingInput,
): Promise<SharingState> {
  return sharingStateSchema.parse(
    await sessionJsonRequest(`/api/v1/systems/${systemId}/sharing`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  );
}
