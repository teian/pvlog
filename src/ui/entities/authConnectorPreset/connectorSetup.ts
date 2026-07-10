import { z } from "zod";

import { AUTH_CONNECTOR_PRESETS } from "./authConnectorPresets";
import type {
  AuthConnectorPresetId,
  ConnectorDraft,
  ConnectorSetupInput,
} from "./authConnectorPreset.types";

const connectorSetupInputSchema = z.object({
  displayName: z.string().trim().min(1).max(80),
  clientId: z.string().trim().min(1).max(512),
  clientSecretReference: z
    .string()
    .trim()
    .min(1)
    .max(1024)
    .refine((value) => !/\s/u.test(value), {
      message: "Secret references must not contain whitespace",
    }),
  redirectUri: z.url().refine(isSafeCallbackUrl, {
    message:
      "Callback URL must use HTTPS, except on a loopback development host",
  }),
});

/**
 * Validates administrator input and materializes a provider-neutral connector draft.
 *
 * @param presetId - Stable preset catalog identifier.
 * @param input - Instance-specific credential references and callback URL.
 * @returns A generic connector draft suitable for the administration API.
 */
export function createConnectorDraft(
  presetId: AuthConnectorPresetId,
  input: ConnectorSetupInput,
): ConnectorDraft {
  const validated = connectorSetupInputSchema.parse(input);
  const preset = AUTH_CONNECTOR_PRESETS.find(
    (candidate) => candidate.id === presetId,
  );
  if (preset === undefined) {
    throw new Error("Unknown authentication connector preset");
  }
  return {
    ...validated,
    preset: { id: preset.id, revision: preset.revision },
    configuration: preset.configuration,
  };
}

function isSafeCallbackUrl(value: string): boolean {
  const callback = new URL(value);
  if (callback.protocol === "https:") {
    return true;
  }
  return (
    callback.protocol === "http:" &&
    (callback.hostname === "127.0.0.1" || callback.hostname === "localhost")
  );
}
