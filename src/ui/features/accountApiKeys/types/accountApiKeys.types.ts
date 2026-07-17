import { z } from "zod";

/** Public least-privilege scopes accepted by account API keys. */
export const accountApiKeyScopeSchema = z.enum([
  "systems:read",
  "systems:write",
  "telemetry:read",
  "telemetry:write",
]);

/** Safe API-key metadata that never contains the cleartext bearer value. */
export const accountApiKeySchema = z.object({
  id: z.uuid(),
  name: z.string().min(1),
  scopes: z.array(accountApiKeyScopeSchema).min(1),
  createdAtEpochMillis: z.number().int(),
  expiresAtEpochMillis: z.number().int().nullable(),
  revokedAtEpochMillis: z.number().int().nullable(),
});

/** One-time API-key creation response. */
export const issuedAccountApiKeySchema = z.object({
  apiKey: z.string().min(1),
  credential: accountApiKeySchema,
});

/** One account API-key action scope. */
export type AccountApiKeyScope = z.infer<typeof accountApiKeyScopeSchema>;
/** Safe account API-key metadata. */
export type AccountApiKey = z.infer<typeof accountApiKeySchema>;

/** Maps public colon-delimited scopes to i18next-safe key segments. @param scope - Public API scope. @returns Translation key segment. */
export function accountApiKeyScopeKey(scope: AccountApiKeyScope): string {
  return {
    "systems:read": "systemsRead",
    "systems:write": "systemsWrite",
    "telemetry:read": "telemetryRead",
    "telemetry:write": "telemetryWrite",
  }[scope];
}
/** Validated API-key creation input. */
export interface CreateAccountApiKeyInput {
  name: string;
  scopes: AccountApiKeyScope[];
  expiresAtEpochMillis: number | null;
}
