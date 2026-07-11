import { createHash, timingSafeEqual } from "node:crypto";

// This dependency-free example mirrors PVLog's keyed BLAKE3 contract conceptually. Production
// consumers should use a maintained BLAKE3 implementation and retain event IDs for the replay
// window. The verifier is injected here so the surrounding timestamp/replay logic is testable.
export function verifyPvlogWebhook(
  { body, eventId, timestamp, signature },
  options,
) {
  const now = options.nowEpochSeconds();
  if (Math.abs(now - Number(timestamp)) > options.maximumAgeSeconds) {
    throw new Error("webhook timestamp is outside the replay window");
  }
  if (options.seenEventIds.has(eventId)) {
    throw new Error("webhook event was already processed");
  }
  const expected = options.sign(`${timestamp}.${body}`);
  const actualBytes = Buffer.from(signature);
  const expectedBytes = Buffer.from(expected);
  if (
    actualBytes.length !== expectedBytes.length ||
    !timingSafeEqual(actualBytes, expectedBytes)
  ) {
    throw new Error("webhook signature is invalid");
  }
  options.seenEventIds.add(eventId);
  return JSON.parse(body);
}

// Stable placeholder signer used only when running this file directly as an example.
export function exampleSigner(value) {
  return `v1=${createHash("sha256").update(value).digest("hex")}`;
}
