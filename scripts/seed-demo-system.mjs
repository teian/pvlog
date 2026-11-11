#!/usr/bin/env bun
/**
 * Seeds a deterministic demo system into one account database.
 *
 * The generated data set contains two inverters (one with two PV strings, one
 * with a single string) and a configurable number of years of five-minute
 * telemetry, together with the derived rollups, daily summaries, and the
 * lifetime summary that the reporting endpoints read.
 *
 * Equipment is expected to exist already; create it through the public API and
 * pass the resulting system identifier. Only telemetry and its aggregates are
 * written here, because the runtime does not currently mount an ingestion
 * router or an aggregation job.
 *
 * Usage:
 *   bun scripts/seed-demo-system.mjs \
 *     --account-db data/accounts/<account>.sqlite3 \
 *     --system-id <uuid-v7> \
 *     [--years 2] [--cadence-minutes 5] [--seed 20260828] [--timezone Europe/Berlin]
 */

import { createHash, randomUUID } from "node:crypto";
import { Database } from "bun:sqlite";

const DEFAULTS = {
  years: 2,
  cadenceMinutes: 5,
  seed: 20_260_828,
  timezone: "Europe/Berlin",
  latitude: 52.52,
  longitude: 13.405,
  sourceIdentity: "seed:demo-two-year",
};

/** String layout mirrored from the equipment created through the public API. */
const STRINGS = [
  { name: "Ost", peakWatts: 8100, orientation: 90, tilt: 30, inverter: 0 },
  { name: "West", peakWatts: 5400, orientation: 270, tilt: 30, inverter: 0 },
  { name: "Sued", peakWatts: 6020, orientation: 180, tilt: 35, inverter: 1 },
];
const INVERTERS = [{ ratedWatts: 10_000 }, { ratedWatts: 5000 }];

/** Deterministic outages expressed as day offsets from the first local day. */
const OUTAGE_DAY_OFFSETS = new Set([137, 138, 431]);

function parseArguments(argv) {
  const options = { ...DEFAULTS };
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    switch (key) {
      case "--account-db":
        options.accountDb = value;
        break;
      case "--system-id":
        options.systemId = value;
        break;
      case "--years":
        options.years = Number(value);
        break;
      case "--cadence-minutes":
        options.cadenceMinutes = Number(value);
        break;
      case "--seed":
        options.seed = Number(value);
        break;
      case "--timezone":
        options.timezone = value;
        break;
      case "--end":
        options.end = value;
        break;
      default:
        throw new Error(`unknown option ${key}`);
    }
  }
  if (!options.accountDb || !options.systemId) {
    throw new Error("--account-db and --system-id are required");
  }
  return options;
}

/** Reproducible 32-bit generator so repeated runs produce identical rows. */
function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d_2b_79_f5) >>> 0;
    let value = Math.imul(state ^ (state >>> 15), 1 | state);
    value = (value + Math.imul(value ^ (value >>> 7), 61 | value)) ^ value;
    return ((value ^ (value >>> 14)) >>> 0) / 4_294_967_296;
  };
}

const partsFormatter = (timezone) =>
  new Intl.DateTimeFormat("en-US", {
    timeZone: timezone,
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

function zonedParts(formatter, epochMillis) {
  const parts = Object.fromEntries(
    formatter
      .formatToParts(new Date(epochMillis))
      .filter((part) => part.type !== "literal")
      .map((part) => [part.type, Number(part.value)]),
  );
  return parts;
}

function zoneOffsetMillis(formatter, epochMillis) {
  const parts = zonedParts(formatter, epochMillis);
  const asUtc = Date.UTC(
    parts.year,
    parts.month - 1,
    parts.day,
    parts.hour % 24,
    parts.minute,
    parts.second,
  );
  return asUtc - epochMillis;
}

/** Resolves one wall-clock instant in the target zone, DST transitions included. */
function zonedEpoch(formatter, year, month, day, hour = 0, minute = 0) {
  const guess = Date.UTC(year, month - 1, day, hour, minute);
  const firstOffset = zoneOffsetMillis(formatter, guess);
  const candidate = guess - firstOffset;
  const secondOffset = zoneOffsetMillis(formatter, candidate);
  return secondOffset === firstOffset ? candidate : guess - secondOffset;
}

const RESOLUTIONS = ["fifteen_minute", "hour", "day", "month", "year"];

const RADIANS = Math.PI / 180;

/** Clear-sky plane-of-array irradiance factor for one string at one instant. */
function irradianceFactor(epochMillis, string_, latitude, longitude) {
  const date = new Date(epochMillis);
  const dayOfYear =
    Math.floor(
      (Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()) -
        Date.UTC(date.getUTCFullYear(), 0, 0)) /
        86_400_000,
    ) || 1;
  const declination =
    23.44 *
    RADIANS *
    Math.sin(((360 / 365) * (dayOfYear - 81) * Math.PI) / 180);
  const utcHours =
    date.getUTCHours() +
    date.getUTCMinutes() / 60 +
    date.getUTCSeconds() / 3600;
  const solarHours = utcHours + longitude / 15;
  const hourAngle = (solarHours - 12) * 15 * RADIANS;
  const latitudeRadians = latitude * RADIANS;

  const sinElevation =
    Math.sin(latitudeRadians) * Math.sin(declination) +
    Math.cos(latitudeRadians) * Math.cos(declination) * Math.cos(hourAngle);
  if (sinElevation <= 0.02) {
    return 0;
  }
  const elevation = Math.asin(sinElevation);
  const azimuth =
    Math.atan2(
      Math.sin(hourAngle),
      Math.cos(hourAngle) * Math.sin(latitudeRadians) -
        Math.tan(declination) * Math.cos(latitudeRadians),
    ) /
      RADIANS +
    180;

  const tilt = string_.tilt * RADIANS;
  const surfaceAzimuth = string_.orientation * RADIANS;
  const incidence =
    Math.sin(elevation) * Math.cos(tilt) +
    Math.cos(elevation) *
      Math.sin(tilt) *
      Math.cos(azimuth * RADIANS - surfaceAzimuth);
  if (incidence <= 0) {
    return 0.04 * sinElevation;
  }
  const airMass =
    1 / (sinElevation + 0.15 * (elevation / RADIANS + 3.885) ** -1.253);
  const atmosphere = 0.7 ** (airMass ** 0.678);
  return incidence * atmosphere;
}

/** Smooth per-day weather factor with occasional overcast stretches. */
function weatherSeries(dayCount, random) {
  const factors = new Array(dayCount);
  let level = 0.75;
  for (let day = 0; day < dayCount; day += 1) {
    const drift = (random() - 0.5) * 0.35;
    level = Math.min(1, Math.max(0.12, level * 0.6 + (0.75 + drift) * 0.4));
    const overcast = random() < 0.08 ? 0.35 : 1;
    factors[day] = Math.min(1, level * overcast);
  }
  return factors;
}

function canonicalHash(systemId, measuredAt, powerWatts, energyWh) {
  return createHash("sha256")
    .update(`${systemId}|${measuredAt}|${powerWatts}|${energyWh}`)
    .digest();
}

function uuidBlob(value) {
  return Buffer.from(value.replaceAll("-", ""), "hex");
}

function main() {
  const options = parseArguments(process.argv.slice(2));
  const formatter = partsFormatter(options.timezone);
  const database = new Database(options.accountDb, { strict: false });
  database.exec("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");

  const systemBlob = uuidBlob(options.systemId);
  const systemRow = database
    .query("SELECT name, timezone FROM systems WHERE id = ?")
    .get(systemBlob);
  if (!systemRow) {
    throw new Error(
      `system ${options.systemId} not found in ${options.accountDb}`,
    );
  }

  const endParts = options.end
    ? options.end.split("-").map(Number)
    : (() => {
        const now = zonedParts(formatter, Date.now());
        return [now.year, now.month, now.day];
      })();
  const endEpoch = zonedEpoch(
    formatter,
    endParts[0],
    endParts[1],
    endParts[2] + 1,
  );
  const startEpoch = zonedEpoch(
    formatter,
    endParts[0] - options.years,
    endParts[1],
    endParts[2] + 1,
  );

  const cadenceMillis = options.cadenceMinutes * 60_000;
  const intervalHours = options.cadenceMinutes / 60;
  const random = mulberry32(options.seed);
  const dayCount = Math.ceil((endEpoch - startEpoch) / 86_400_000) + 2;
  const weather = weatherSeries(dayCount, random);

  const insertObservation = database.prepare(
    "INSERT OR REPLACE INTO telemetry_hot (observation_id,system_id,measured_at,received_at,source_kind,source_identity,idempotency_identity,quality_flags,generation_power_watts,generation_energy_wh,consumption_power_watts,consumption_energy_wh,provenance_json,canonical_hash,version) VALUES (?,?,?,?,'import',?,?,0,?,?,?,?,'{\"seed\":\"demo\"}',?,1)",
  );

  const buckets = {
    fifteen_minute: new Map(),
    hour: new Map(),
    day: new Map(),
    month: new Map(),
    year: new Map(),
  };
  const daily = new Map();
  const lifetime = {
    generationWh: 0,
    consumptionWh: 0,
    peakWatts: 0,
    peakAt: null,
    first: null,
    last: null,
    points: 0,
    expected: 0,
  };

  const bucketDescriptor = (resolution, parts) => {
    switch (resolution) {
      case "fifteen_minute": {
        const minute = Math.floor(parts.minute / 15) * 15;
        return {
          key: `${parts.year}-${parts.month}-${parts.day}T${parts.hour}:${minute}`,
          start: () =>
            zonedEpoch(
              formatter,
              parts.year,
              parts.month,
              parts.day,
              parts.hour % 24,
              minute,
            ),
          width: 900_000,
        };
      }
      case "hour":
        return {
          key: `${parts.year}-${parts.month}-${parts.day}T${parts.hour}`,
          start: () =>
            zonedEpoch(
              formatter,
              parts.year,
              parts.month,
              parts.day,
              parts.hour % 24,
            ),
          width: 3_600_000,
        };
      case "day":
        return {
          key: `${parts.year}-${parts.month}-${parts.day}`,
          start: () =>
            zonedEpoch(formatter, parts.year, parts.month, parts.day),
          width: null,
        };
      case "month":
        return {
          key: `${parts.year}-${parts.month}`,
          start: () => zonedEpoch(formatter, parts.year, parts.month, 1),
          width: null,
        };
      default:
        return {
          key: `${parts.year}`,
          start: () => zonedEpoch(formatter, parts.year, 1, 1),
          width: null,
        };
    }
  };

  const accumulate = (
    resolution,
    parts,
    energyWh,
    consumptionWh,
    powerWatts,
    covered,
  ) => {
    const descriptor = bucketDescriptor(resolution, parts);
    let bucket = buckets[resolution].get(descriptor.key);
    if (!bucket) {
      const bucketStart = descriptor.start();
      bucket = {
        bucketStart,
        bucketEnd: descriptor.width ? bucketStart + descriptor.width : null,
        energyWh: 0,
        consumptionWh: 0,
        minWatts: null,
        maxWatts: null,
        firstWatts: null,
        lastWatts: null,
        points: 0,
        expected: 0,
      };
      buckets[resolution].set(descriptor.key, bucket);
    }
    bucket.expected += 1;
    if (!covered) {
      return;
    }
    bucket.points += 1;
    bucket.energyWh += energyWh;
    bucket.consumptionWh += consumptionWh;
    bucket.minWatts =
      bucket.minWatts === null
        ? powerWatts
        : Math.min(bucket.minWatts, powerWatts);
    bucket.maxWatts =
      bucket.maxWatts === null
        ? powerWatts
        : Math.max(bucket.maxWatts, powerWatts);
    bucket.firstWatts =
      bucket.firstWatts === null ? powerWatts : bucket.firstWatts;
    bucket.lastWatts = powerWatts;
  };

  const writeObservations = database.transaction(() => {
    for (
      let epochMillis = startEpoch;
      epochMillis < endEpoch;
      epochMillis += cadenceMillis
    ) {
      const dayOffset = Math.floor((epochMillis - startEpoch) / 86_400_000);
      const covered = !OUTAGE_DAY_OFFSETS.has(dayOffset);
      const cloudFactor = weather[dayOffset] ?? 0.75;

      let acWatts = 0;
      for (const [index, inverter] of INVERTERS.entries()) {
        let dcWatts = 0;
        for (const string_ of STRINGS.filter(
          (entry) => entry.inverter === index,
        )) {
          dcWatts +=
            string_.peakWatts *
            irradianceFactor(
              epochMillis,
              string_,
              options.latitude,
              options.longitude,
            ) *
            cloudFactor;
        }
        const efficiency = dcWatts > 0 ? 0.955 : 0;
        acWatts += Math.min(inverter.ratedWatts, dcWatts * efficiency);
      }

      const generationWatts = Math.max(0, Math.round(acWatts));
      const generationEnergyWh = Math.round(generationWatts * intervalHours);
      const baseLoad = 220 + Math.sin((epochMillis / 3_600_000) * 0.26) * 90;
      const consumptionWatts = Math.max(
        60,
        Math.round(baseLoad + random() * 140),
      );
      const consumptionEnergyWh = Math.round(consumptionWatts * intervalHours);

      const parts = zonedParts(formatter, epochMillis);
      for (const resolution of RESOLUTIONS) {
        accumulate(
          resolution,
          parts,
          generationEnergyWh,
          consumptionEnergyWh,
          generationWatts,
          covered,
        );
      }

      const dateKey = `${parts.year}-${String(parts.month).padStart(2, "0")}-${String(parts.day).padStart(2, "0")}`;
      let summary = daily.get(dateKey);
      if (!summary) {
        summary = {
          generationWh: 0,
          consumptionWh: 0,
          peakWatts: 0,
          peakAt: null,
          points: 0,
          expected: 0,
        };
        daily.set(dateKey, summary);
      }
      summary.expected += 1;
      lifetime.expected += 1;

      if (!covered) {
        continue;
      }

      insertObservation.run(
        uuidBlob(randomUUID()),
        systemBlob,
        epochMillis,
        epochMillis + 1500,
        options.sourceIdentity,
        `${options.sourceIdentity}:${epochMillis}`,
        generationWatts,
        generationEnergyWh,
        consumptionWatts,
        consumptionEnergyWh,
        canonicalHash(
          options.systemId,
          epochMillis,
          generationWatts,
          generationEnergyWh,
        ),
      );

      summary.points += 1;
      summary.generationWh += generationEnergyWh;
      summary.consumptionWh += consumptionEnergyWh;
      if (generationWatts > summary.peakWatts) {
        summary.peakWatts = generationWatts;
        summary.peakAt = epochMillis;
      }

      lifetime.points += 1;
      lifetime.generationWh += generationEnergyWh;
      lifetime.consumptionWh += consumptionEnergyWh;
      lifetime.first ??= epochMillis;
      lifetime.last = epochMillis;
      if (generationWatts > lifetime.peakWatts) {
        lifetime.peakWatts = generationWatts;
        lifetime.peakAt = epochMillis;
      }
    }
  });

  const calculatedAt = Date.now();
  const insertRollup = database.prepare(
    "INSERT OR REPLACE INTO telemetry_rollups (system_id,scope_kind,scope_id,resolution,bucket_start,bucket_end,timezone,generation,source_generation,point_count,expected_count,generation_energy_sum_wh,consumption_energy_sum_wh,generation_power_min_watts,generation_power_max_watts,generation_power_first_watts,generation_power_last_watts,quality_flags,coverage_basis_points,calculated_at) VALUES (?,'system',?,?,?,?,?,1,1,?,?,?,?,?,?,?,?,0,?,?)",
  );
  const insertDaily = database.prepare(
    "INSERT OR REPLACE INTO system_daily_summaries (system_id,local_date,timezone,generation,generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,peak_generation_at,coverage_basis_points,quality_flags,calculated_at) VALUES (?,?,?,1,?,?,?,?,?,0,?)",
  );
  const insertLifetime = database.prepare(
    "INSERT OR REPLACE INTO system_lifetime_summaries (system_id,generation,first_observation_at,last_observation_at,generation_energy_wh,consumption_energy_wh,peak_generation_power_watts,peak_generation_at,coverage_basis_points,calculated_at) VALUES (?,1,?,?,?,?,?,?,?,?)",
  );

  const coverage = (points, expected) =>
    expected === 0
      ? 0
      : Math.min(10_000, Math.round((points / expected) * 10_000));

  const writeAggregates = database.transaction(() => {
    for (const [resolution, entries] of Object.entries(buckets)) {
      const ordered = [...entries.values()].sort(
        (left, right) => left.bucketStart - right.bucketStart,
      );
      for (const [index, bucket] of ordered.entries()) {
        const nextStart = ordered[index + 1]?.bucketStart;
        const bucketEnd =
          bucket.bucketEnd ??
          nextStart ??
          bucket.bucketStart + bucket.expected * cadenceMillis;
        insertRollup.run(
          systemBlob,
          systemBlob,
          resolution,
          bucket.bucketStart,
          bucketEnd,
          options.timezone,
          bucket.points,
          bucket.expected,
          bucket.energyWh,
          bucket.consumptionWh,
          bucket.minWatts,
          bucket.maxWatts,
          bucket.firstWatts,
          bucket.lastWatts,
          coverage(bucket.points, bucket.expected),
          calculatedAt,
        );
      }
    }
    for (const [localDate, summary] of daily) {
      insertDaily.run(
        systemBlob,
        localDate,
        options.timezone,
        summary.generationWh,
        summary.consumptionWh,
        summary.peakWatts,
        summary.peakAt,
        coverage(summary.points, summary.expected),
        calculatedAt,
      );
    }
    insertLifetime.run(
      systemBlob,
      lifetime.first,
      lifetime.last,
      lifetime.generationWh,
      lifetime.consumptionWh,
      lifetime.peakWatts,
      lifetime.peakAt,
      coverage(lifetime.points, lifetime.expected),
      calculatedAt,
    );
  });

  writeObservations();
  writeAggregates();
  database.close();

  const summaryReport = {
    system: systemRow.name,
    timezone: options.timezone,
    rangeStart: new Date(startEpoch).toISOString(),
    rangeEnd: new Date(endEpoch).toISOString(),
    cadenceMinutes: options.cadenceMinutes,
    observations: lifetime.points,
    outageDays: OUTAGE_DAY_OFFSETS.size,
    generationKwh: Math.round(lifetime.generationWh / 1000),
    peakWatts: lifetime.peakWatts,
    coverageBasisPoints: coverage(lifetime.points, lifetime.expected),
    rollups: Object.fromEntries(
      Object.entries(buckets).map(([resolution, entries]) => [
        resolution,
        entries.size,
      ]),
    ),
    dailySummaries: daily.size,
  };
  process.stdout.write(`${JSON.stringify(summaryReport, null, 2)}\n`);
}

main();
