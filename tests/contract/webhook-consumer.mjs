import assert from "node:assert/strict";
import {
  exampleSigner,
  verifyPvlogWebhook,
} from "../../docs/examples/webhook-consumer.mjs";

const body = JSON.stringify({ schema_version: 1, event_id: "event-1" });
const timestamp = "1700000000";
const seenEventIds = new Set();
const input = {
  body,
  eventId: "event-1",
  timestamp,
  signature: exampleSigner(`${timestamp}.${body}`),
};
const options = {
  nowEpochSeconds: () => 1700000001,
  maximumAgeSeconds: 300,
  seenEventIds,
  sign: exampleSigner,
};
assert.equal(verifyPvlogWebhook(input, options).schema_version, 1);
assert.throws(() => verifyPvlogWebhook(input, options), /already processed/);
console.log("Webhook consumer example: signature and replay guidance verified");
