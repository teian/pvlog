import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { parse } from "yaml";

const document = parse(readFileSync("openapi/pvlog-v1.yaml", "utf8"));
const paths = document.paths;

const series = paths["/api/v1/systems/{system_id}/series"].get;
const seriesExample =
  series.responses["200"].content["application/json"].examples.dstDay.value;
assert.equal(seriesExample.timezone, "Europe/Berlin");
assert.equal(seriesExample.actualResolution, "hourly");
assert.ok(
  paths["/api/v1/systems/{system_id}/series"].parameters.some((parameter) =>
    parameter.$ref?.endsWith("/MaximumPoints"),
  ),
);
assert.ok(
  series.responses["304"],
  "series query must document conditional caching",
);

const exportOperation =
  paths["/api/v1/systems/{system_id}/analysis-exports"].post;
const examples =
  exportOperation.requestBody.content["application/json"].examples;
assert.equal(examples.csv.value.format, "csv");
assert.equal(examples.csv.value.timezone, "Europe/Berlin");
assert.equal(examples.queuedJson.value.asynchronous, true);
assert.ok(exportOperation.responses["202"], "large exports must document jobs");

const csv = exportOperation.responses["200"].content["text/csv"].example;
assert.equal(
  csv.split("\n", 1)[0],
  "timestamp_epoch_millis,field,value,unit,coverage_basis_points,quality_flags,resolution,timezone",
);

const qualityKinds =
  document.components.schemas.DataQualityIssue.properties.kind.enum;
for (const kind of [
  "missing_interval",
  "suspect_observation",
  "source_conflict",
  "counter_reset",
  "rejected_ingestion",
  "aggregate_lag",
]) {
  assert.ok(qualityKinds.includes(kind), `missing data-quality kind: ${kind}`);
}

console.log("OpenAPI analytics examples: validated");
