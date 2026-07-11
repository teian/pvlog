import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const report = JSON.parse(
  readFileSync("tests/fixtures/performance/scale-workload-report.json", "utf8"),
);
assert.equal(report.schemaVersion, 1);
for (const field of [
  "recordedAt",
  "hardware",
  "postgresqlSettings",
  "bytesPerSystemDay",
  "compressionRatio",
  "queueLagSeconds",
]) {
  assert.ok(report[field] !== undefined, `missing ${field}`);
}
assert.equal(report.workloads.length, 2);
for (const workload of report.workloads) {
  assert.ok(workload.concurrency > 0);
  assert.ok(workload.latenciesMillis.length >= 20);
  const sorted = workload.latenciesMillis.toSorted((a, b) => a - b);
  const percentile = (value) =>
    sorted[Math.ceil((value / 100) * sorted.length) - 1];
  const summary = {
    p50: percentile(50),
    p95: percentile(95),
    p99: percentile(99),
  };
  console.log(`${workload.name}: ${JSON.stringify(summary)}`);
}
const ingestion = report.workloads.find(
  ({ name }) => name === "burst-ingestion",
);
assert.ok(ingestion.observationsPerSecond >= 250);
