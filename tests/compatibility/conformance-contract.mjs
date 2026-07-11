import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

const directory = "tests/fixtures/pvoutput";
const inventory = JSON.parse(
  readFileSync(`${directory}/r2-inventory-2026-07-11.json`, "utf8"),
);
const conformance = JSON.parse(
  readFileSync(`${directory}/r2-conformance-2026-07-11.json`, "utf8"),
);

const inventoryRoutes = inventory.services.map(({ route }) => route).sort();
const records = conformance.routes;
assert.equal(records.length, 21);
assert.deepEqual(
  records.map(({ route }) => route).sort(),
  inventoryRoutes,
  "conformance routes must exactly match the dated inventory",
);

for (const record of records) {
  assert.ok(
    ["passing", "intentional_difference"].includes(record.status),
    `${record.route} has an invalid status`,
  );
  if (record.status === "passing") {
    assert.ok(record.evidence, `${record.route} has no passing-test evidence`);
    assert.ok(existsSync(record.evidence), `${record.evidence} does not exist`);
  } else {
    assert.match(record.reason ?? "", /OpenSpec task \d+\.\d+/);
  }
}

console.log(
  `PVOutput conformance: ${records.filter(({ status }) => status === "passing").length} passing, ${records.filter(({ status }) => status === "intentional_difference").length} documented intentional differences`,
);
