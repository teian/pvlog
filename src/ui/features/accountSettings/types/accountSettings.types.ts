import { z } from "zod";

/** Safe current-user profile returned by the account endpoint. */
export const accountProfileSchema = z.object({
  id: z.uuid(),
  email: z.email(),
  displayName: z.string().min(1).max(120),
});

/** Current-user account profile. */
export type AccountProfile = z.infer<typeof accountProfileSchema>;

/** Allowed self-service profile mutation. */
export interface UpdateAccountProfileInput {
  displayName: string;
}

/** Password-change input sent only to the local password endpoint. */
export interface ChangePasswordInput {
  currentPassword: string;
  newPassword: string;
}
