import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const fixturePath =
  "tests/fixtures/performance/fleet-history-generator-v1.json";
const requiredScenarios = new Set([
  "sparseExtendedChannels",
  "denseExtendedChannels",
  "irregularIntervals",
  "dstSpring",
  "dstFall",
  "counterReset",
  "gap",
  "correction",
  "segmentedTwentyFiveYears",
]);

/** Builds a deterministic, lazy data-set manifest from a reusable fixture. @param fixture - Scenario configuration. @returns A stable manifest with sample observations and segment plans. */
export function generateFleetHistory(fixture) {
  const systems = generateSystems(fixture.fleet);
  const scenarios = fixture.scenarios.map((scenario) => ({
    name: scenario.name,
    observations: generateScenario(scenario, systems[0]),
  }));
  return { schemaVersion: fixture.schemaVersion, systems, scenarios };
}

/** Produces reproducible opaque system identifiers without relying on random UUID state. @param fleet - Fleet count and seed. @returns Ordered generated system identities. */
export function generateSystems(fleet) {
  const random = seededRandom(fleet.seed);
  return Array.from({ length: fleet.systemCount }, (_, index) => ({
    id: `system-${String(index + 1).padStart(5, "0")}`,
    capacityWatts: 3_000 + Math.floor(random() * 7_000),
    timezone: index % 2 === 0 ? "Europe/Berlin" : "Australia/Sydney",
  }));
}

function generateScenario(scenario, system) {
  switch (scenario.name) {
    case "sparseExtendedChannels":
      return boundedObservations(scenario, system, (index) => ({
        extended: index % 3 === 0 ? { irradianceWattsPerSquareMetre: 640 } : {},
      }));
    case "denseExtendedChannels":
      return boundedObservations(scenario, system, () => ({
        extended: {
          batteryMillivolts: 52_100,
          irradianceWattsPerSquareMetre: 640,
          inverterTemperatureMilliCelsius: 31_500,
        },
      }));
    case "irregularIntervals":
      return boundedObservations(scenario, system, (index) => ({
        measuredAtEpochMillis:
          scenario.startEpochMillis +
          irregularOffset(index, scenario.cadenceMillis),
      }));
    case "dstSpring":
    case "dstFall":
      return boundedObservations(scenario, system, (index) => ({
        localTimezone: "Europe/Berlin",
        measuredAtEpochMillis:
          scenario.startEpochMillis + index * scenario.cadenceMillis,
      }));
    case "counterReset":
      return boundedObservations(scenario, system, (index) => ({
        cumulativeEnergyWh: index < 4 ? 10_000 + index * 80 : (index - 4) * 80,
      }));
    case "gap":
      return boundedObservations(scenario, system, (index) => ({
        quality: index === 3 ? "missing_interval" : "accepted",
      })).filter((_, index) => index !== 3);
    case "correction":
      return boundedObservations(scenario, system, (index) => ({
        correctionOf: index === 5 ? "observation-0005" : undefined,
        quality: index === 5 ? "corrected" : "accepted",
      }));
    case "segmentedTwentyFiveYears":
      return segmentPlan(scenario, system);
    default:
      throw new Error(`unsupported performance scenario: ${scenario.name}`);
  }
}

function boundedObservations(scenario, system, patch) {
  return Array.from({ length: scenario.samples }, (_, index) => ({
    id: `observation-${String(index).padStart(4, "0")}`,
    systemId: system.id,
    measuredAtEpochMillis:
      scenario.startEpochMillis + index * scenario.cadenceMillis,
    generationWatts: 1_000 + index * 50,
    quality: "accepted",
    ...patch(index),
  }));
}

function segmentPlan(scenario, system) {
  const days = scenario.years * 365 + scenario.leapDays;
  return Array.from({ length: days }, (_, day) => ({
    systemId: system.id,
    day,
    pointCount: scenario.pointsPerDay,
    startEpochMillis: scenario.startEpochMillis + day * 86_400_000,
    contentSeed: stableDigest(`${system.id}:${day}:${scenario.seed}`),
  }));
}

function irregularOffset(index, cadenceMillis) {
  const pattern = [0, 0, cadenceMillis / 5, -cadenceMillis / 10];
  return index * cadenceMillis + pattern[index % pattern.length];
}

function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1_664_525 + 1_013_904_223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

function stableDigest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function manifestDigest(manifest) {
  return stableDigest(JSON.stringify(manifest));
}

function verify() {
  const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
  const names = new Set(fixture.scenarios.map((scenario) => scenario.name));
  assert.deepEqual(names, requiredScenarios);
  const manifest = generateFleetHistory(fixture);
  assert.equal(manifest.systems.length, fixture.fleet.systemCount);
  assert.equal(manifest.scenarios.length, requiredScenarios.size);
  const longHistory = manifest.scenarios.find(
    (scenario) => scenario.name === "segmentedTwentyFiveYears",
  );
  const segmentFixture = fixture.scenarios.find(
    (scenario) => scenario.name === "segmentedTwentyFiveYears",
  );
  const expectedDays = segmentFixture.years * 365 + segmentFixture.leapDays;
  assert.equal(longHistory.observations.length, expectedDays);
  assert.equal(longHistory.observations[0].pointCount, 288);
  assert.equal(longHistory.observations.at(-1).day, expectedDays - 1);
  const gap = manifest.scenarios.find((scenario) => scenario.name === "gap");
  assert.equal(gap.observations.length, 7);
  const correction = manifest.scenarios.find(
    (scenario) => scenario.name === "correction",
  );
  assert.equal(correction.observations[5].quality, "corrected");
  assert.equal(manifestDigest(manifest), fixture.expectedManifestSha256);
  console.log(`fleet-history-generator: ${fixture.expectedManifestSha256}`);
}

if (process.argv[1]?.endsWith("fleet-history-generator.mjs")) verify();
