import { z } from "zod";

const resultSchema = z.object({
  systemId: z.uuid(),
  credentialSecret: z.string().min(1),
  testEndpoint: z.url(),
});
const verificationSchema = z.object({
  accepted: z.boolean(),
  observedAtEpochMillis: z.number().int(),
});
/** Onboarding values submitted to the backend. */
export interface OnboardingInput {
  instanceName: string;
  systemName: string;
  capacityWatts: number;
  timezone: string;
  equipmentName: string;
  credentialName: string;
}
/** Successful onboarding response with the one-time credential. */
export type OnboardingResult = z.infer<typeof resultSchema>;
/** Creates instance metadata, first system, equipment, and credential atomically. @param input - Validated onboarding values. @returns The created system and one-time credential. */
export async function createOnboarding(
  input: OnboardingInput,
): Promise<OnboardingResult> {
  const response = await fetch("/api/v1/onboarding", {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!response.ok) throw new Error("onboarding_failed");
  return resultSchema.parse(await response.json());
}
/** Sends a deterministic test observation. @param result - Created endpoint and credential. @returns Completion after acceptance. */
export async function sendTestObservation(
  result: OnboardingResult,
): Promise<void> {
  const response = await fetch(result.testEndpoint, {
    method: "POST",
    headers: {
      authorization: `Bearer ${result.credentialSecret}`,
      "content-type": "application/json",
      "idempotency-key": "onboarding-test",
    },
    body: JSON.stringify({
      observedAtEpochMillis: Date.now(),
      generationPowerWatts: 100,
    }),
  });
  if (!response.ok) throw new Error("test_ingestion_failed");
}
/** Verifies canonical persistence. @param systemId - Created system identifier. @returns Verification state. */
export async function verifyTestObservation(systemId: string) {
  const response = await fetch(
    `/api/v1/systems/${systemId}/onboarding-verification`,
    { credentials: "same-origin" },
  );
  if (!response.ok) throw new Error("verification_failed");
  return verificationSchema.parse(await response.json());
}
