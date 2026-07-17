import {
  accountProfileSchema,
  type AccountProfile,
  type ChangePasswordInput,
  type UpdateAccountProfileInput,
} from "@/features/accountSettings/types/accountSettings.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";

/** Loads safe profile data for the current browser user. @returns Validated profile data. */
export async function fetchAccountProfile(): Promise<AccountProfile> {
  return accountProfileSchema.parse(
    await sessionJsonRequest("/api/v1/account/profile"),
  );
}

/** Updates the current user's display name. @param input - Allowed profile fields. @returns Updated validated profile. */
export async function updateAccountProfile(
  input: UpdateAccountProfileInput,
): Promise<AccountProfile> {
  return accountProfileSchema.parse(
    await sessionJsonRequest("/api/v1/account/profile", {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  );
}

/** Changes the current user's local password after current-password verification. @param input - Current and replacement password. @returns Completion after rotation. */
export async function changeAccountPassword(
  input: ChangePasswordInput,
): Promise<void> {
  await sessionJsonRequest("/api/v1/auth/password", {
    method: "PUT",
    body: JSON.stringify(input),
  });
}
