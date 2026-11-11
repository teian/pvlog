import { z } from "zod";

/** Sharing state of one system as its owner sees it. */
export const sharingStateSchema = z.object({
  systemId: z.uuid(),
  linkEnabled: z.boolean(),
  publicToken: z.string().nullable().default(null),
  visibility: z.enum(["private", "unlisted", "public"]),
});

export type SharingState = z.infer<typeof sharingStateSchema>;

/**
 * Requested change to a system's sharing.
 *
 * @property rotateToken - Issues a new link and invalidates the one handed out before.
 */
export interface SharingInput {
  linkEnabled: boolean;
  rotateToken?: boolean;
  visibility?: SharingState["visibility"];
}
