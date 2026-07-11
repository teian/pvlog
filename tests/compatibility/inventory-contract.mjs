import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const inventory = JSON.parse(
  readFileSync("tests/fixtures/pvoutput/r2-inventory-2026-07-11.json", "utf8"),
);

assert.equal(inventory.schemaVersion, 1);
assert.equal(
  inventory.sourceUrl,
  "https://pvoutput.org/help/api_specification.html",
);
assert.match(inventory.retrievedAt, /^\d{4}-\d{2}-\d{2}$/);
assert.match(inventory.sourceSha256, /^[0-9a-f]{64}$/);
assert.equal(inventory.services.length, 21);
assert.equal(
  new Set(inventory.services.map((service) => service.route)).size,
  21,
);

for (const service of inventory.services) {
  assert.match(service.route, /^\/service\/r2\/[a-z]+\.jsp$/);
  assert.ok(service.methods.length > 0, `${service.route} has no method`);
  assert.ok(service.sections.length > 0, `${service.route} has no sections`);
  assert.ok(
    service.parameterTables.some((table) => table.rows.length > 0),
    `${service.route} has no parameter inventory`,
  );
  assert.ok(Array.isArray(service.errors));
  assert.ok(Array.isArray(service.restrictions));
  assert.ok(Array.isArray(service.donationFeatures));
}

for (const common of [
  "Getting Started",
  "Rate Limits",
  "HTTP Headers",
  "Common Errors",
]) {
  assert.ok(
    inventory.commonContract[common],
    `missing common contract: ${common}`,
  );
}

const matrix = readFileSync("docs/compatibility/pvoutput-r2-matrix.md", "utf8");
for (const service of inventory.services) {
  assert.ok(matrix.includes(`\`${service.route}\``));
}

console.log("PVOutput inventory contract: 21 services validated");
