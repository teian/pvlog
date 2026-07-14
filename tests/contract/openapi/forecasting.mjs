import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { parse } from "yaml";

const document = parse(readFileSync("openapi/pvlog-v1.yaml", "utf8"));
const paths = document.paths;

const scopedSettings = [
  "/api/v1/accounts/{account_id}/forecast-settings",
  "/api/v1/accounts/{account_id}/systems/{system_id}/forecast-settings",
  "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/forecast-settings",
  "/api/v1/accounts/{account_id}/systems/{system_id}/inverters/{inverter_id}/strings/{string_id}/forecast-settings",
];
for (const path of scopedSettings) {
  const resource = paths[path];
  assert.ok(resource.get, `${path} must expose effective settings`);
  assert.ok(resource.put, `${path} must expose settings updates`);
  assert.ok(
    resource.put.parameters.some((parameter) =>
      parameter.$ref?.endsWith("/IfMatch"),
    ),
    `${path} must require If-Match`,
  );
  assert.ok(resource.put.responses["412"]);
  assert.ok(resource.put.responses["422"]);
  assert.ok(resource.put.responses["428"]);
  assert.equal(resource.put.security.length, 2);
}

const runs =
  paths["/api/v1/accounts/{account_id}/systems/{system_id}/forecast-runs"].get;
const runParameters =
  paths["/api/v1/accounts/{account_id}/systems/{system_id}/forecast-runs"]
    .parameters;
assert.equal(
  runParameters.find((parameter) => parameter.name === "limit").schema.maximum,
  100,
);
assert.ok(runs.responses["503"], "provider unavailability must be explicit");

const yieldPath =
  paths["/api/v1/accounts/{account_id}/systems/{system_id}/yield-series"];
assert.ok(
  yieldPath.parameters.some((parameter) =>
    parameter.$ref?.endsWith("/YieldMaximumPoints"),
  ),
);
assert.ok(yieldPath.get.responses["413"]);
assert.ok(yieldPath.get.responses["503"]);
const forecastExample =
  yieldPath.get.responses["200"].content["application/json"].examples
    .partialForecast.value;
assert.equal(forecastExample.freshness, "fresh");
assert.equal(forecastExample.includedCapacityWatts, 4000);
assert.equal(forecastExample.totalEffectiveCapacityWatts, 8000);

const performance =
  paths["/api/v1/accounts/{account_id}/systems/{system_id}/yield-performance"]
    .get;
assert.match(performance.description, /never allocated downward/);
assert.ok(performance.responses["422"]);

const reasons = document.components.schemas.ForecastCompletenessReason.enum;
for (const reason of [
  "missing_weather_input",
  "partial_effective_capacity",
  "insufficient_actual_coverage",
  "non_positive_expected_energy",
]) {
  assert.ok(reasons.includes(reason), `missing forecast reason ${reason}`);
}
assert.ok(document.components.responses.ForecastValidationProblem);
assert.ok(document.components.responses.ForecastUnavailableProblem);
assert.ok(document.components.responses.QueryTooLargeProblem);

console.log("OpenAPI forecasting contract: validated");
