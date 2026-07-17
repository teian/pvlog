import {
  accountApiKeySchema,
  issuedAccountApiKeySchema,
  type AccountApiKey,
  type CreateAccountApiKeyInput,
} from "@/features/accountApiKeys/types/accountApiKeys.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

/** Lists safe metadata for the current user's account API keys. @returns Validated key metadata without secrets. */
export async function fetchAccountApiKeys(): Promise<AccountApiKey[]> {
  return z
    .array(accountApiKeySchema)
    .parse(await sessionJsonRequest("/api/v1/account/api-keys"));
}

/** Creates an account API key and returns its one-time secret response. @param input - Name, explicit scopes, and optional expiry. @returns Validated one-time response. */
export async function createAccountApiKey(input: CreateAccountApiKeyInput) {
  return issuedAccountApiKeySchema.parse(
    await sessionJsonRequest("/api/v1/account/api-keys", {
      method: "POST",
      body: JSON.stringify(input),
    }),
  );
}

/** Revokes one current-account API key. @param id - Safe credential identifier, not the secret. @returns Completion after revocation. */
export async function revokeAccountApiKey(id: string): Promise<void> {
  await sessionJsonRequest(`/api/v1/account/api-keys/${id}`, {
    method: "DELETE",
  });
}
