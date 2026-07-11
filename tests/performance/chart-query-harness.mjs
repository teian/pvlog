import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const reportPath =
  process.argv[2] ?? "tests/fixtures/performance/chart-query-regression.json";
const report = JSON.parse(readFileSync(reportPath, "utf8"));

assert.equal(report.schemaVersion, 1);
assert.ok(
  report.recordedAt,
  "report must record when measurements were captured",
);
assert.ok(report.hardware, "report must identify the measurement environment");
assert.equal(report.workloads.length, 2);

const expected = new Map([
  ["30-day-single-system", { rangeDays: 30, target: 500 }],
  ["25-year-daily-single-system", { rangeDays: 9125, target: 1000 }],
]);

for (const workload of report.workloads) {
  const objective = expected.get(workload.name);
  assert.ok(objective, `unknown chart workload: ${workload.name}`);
  assert.equal(workload.rangeDays, objective.rangeDays);
  assert.equal(workload.targetP95Millis, objective.target);
  assert.ok(workload.requestedMaximumPoints > 0);
  assert.ok(workload.expectedResolution);
  assert.ok(
    workload.latenciesMillis.length >= 20,
    `${workload.name} requires at least 20 samples`,
  );
  assert.ok(
    workload.latenciesMillis.every(
      (latency) => Number.isFinite(latency) && latency >= 0,
    ),
  );
  const sorted = workload.latenciesMillis.toSorted(
    (left, right) => left - right,
  );
  const summary = {
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
  };
  assert.ok(
    summary.p95 < objective.target,
    `${workload.name} p95 ${summary.p95}ms must be below ${objective.target}ms`,
  );
  console.log(`${workload.name}: ${JSON.stringify(summary)} ms`);
}

function percentile(sorted, percentage) {
  const rank = Math.ceil((percentage / 100) * sorted.length);
  return sorted[Math.max(0, rank - 1)];
}
