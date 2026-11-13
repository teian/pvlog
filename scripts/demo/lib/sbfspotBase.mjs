/**
 * Turns a copy of a real SBFspot database into the base of the demo data:
 * moved by whole years so that it reaches up to now with the seasons in
 * place, completed with readings at night, and given a household's
 * consumption. The original database is never opened for writing.
 */

import { Database } from "bun:sqlite";
import { copyFileSync } from "node:fs";

import { householdPowerW } from "./household.mjs";
import { localTime } from "./localTime.mjs";

const TABLES_WITH_TIME = ["SpotData", "DayData", "MonthData", "EventData"];
const STEP_SECONDS = 300;

/**
 * Copies the SBFspot database without its write-ahead log. A log copied from
 * a running or synchronised installation can be older than the database and
 * would then corrupt the copy.
 *
 * @param {string} source
 * @param {string} target
 */
export function copyDatabase(source, target) {
  copyFileSync(source, target);
  const database = new Database(target);
  const [check] = database.query("PRAGMA integrity_check").values();
  database.close();
  if (check?.[0] !== "ok")
    throw new Error(`${source} is damaged: ${String(check?.[0])}`);
}

/**
 * How many whole years the data moves so that its end reaches up to now.
 *
 * @param {number} lastEpochSeconds
 * @param {number} nowEpochSeconds
 * @returns {number}
 */
export function yearsToShift(lastEpochSeconds, nowEpochSeconds) {
  let years = 0;
  const last = new Date(lastEpochSeconds * 1000);
  while (true) {
    const moved = new Date(last);
    moved.setUTCFullYear(last.getUTCFullYear() + years);
    if (moved.getTime() / 1000 >= nowEpochSeconds - 86_400) return years;
    years += 1;
  }
}

/**
 * Moves every time of the database by whole calendar years and drops what
 * then lies in the future. A 29 February that does not exist in the target
 * year is dropped. Instants move in UTC, so the sun keeps its position.
 *
 * @param {Database} database
 * @param {number} years
 * @param {number} nowEpochSeconds
 */
function shift(database, years, nowEpochSeconds) {
  for (const table of [...TABLES_WITH_TIME, "Inverters"]) {
    if (years > 0) {
      if (years % 4 !== 0 && table !== "Inverters")
        database.run(
          `DELETE FROM ${table} WHERE strftime('%m-%d', TimeStamp, 'unixepoch') = '02-29'`,
        );
      // Negate first so that moved rows never meet rows not yet moved.
      database.run(`UPDATE ${table} SET TimeStamp = -TimeStamp`);
      database.run(
        `UPDATE ${table} SET TimeStamp = CAST(strftime('%s', -TimeStamp, 'unixepoch', '+${years} years') AS INTEGER)`,
      );
    }
    // The identity of an inverter stays; only its last contact moves back.
    if (table === "Inverters")
      database.run(`UPDATE Inverters SET TimeStamp = ?1 WHERE TimeStamp > ?1`, [
        nowEpochSeconds,
      ]);
    else
      database.run(`DELETE FROM ${table} WHERE TimeStamp > ?`, [
        nowEpochSeconds,
      ]);
  }
}

/**
 * SBFspot archives readings while the inverters feed in. The demo needs the
 * night too, so that consumption is known around the clock: on every (UTC)
 * day with readings, the archive times before the first and after the last
 * reading get a row per inverter with no power and the counter it last had.
 * Days without any reading stay empty, so an outage remains a gap.
 *
 * @param {Database} database
 * @returns {number} Rows added.
 */
function fillNights(database) {
  database.run(
    "CREATE INDEX IF NOT EXISTS demo_daydata_serial ON DayData(Serial, TimeStamp)",
  );
  const [first, last] = database
    .query("SELECT MIN(TimeStamp), MAX(TimeStamp) FROM DayData")
    .values()[0];
  const before = database.query("SELECT COUNT(*) FROM DayData").values()[0][0];
  database.run(
    `WITH RECURSIVE times(t) AS (
       SELECT ?1 UNION ALL SELECT t + ${STEP_SECONDS} FROM times WHERE t < ?2
     ),
     bounds AS (
       SELECT TimeStamp / 86400 AS day, MIN(TimeStamp) AS low, MAX(TimeStamp) AS high
       FROM DayData GROUP BY day
     ),
     empty AS (
       SELECT t FROM times JOIN bounds ON bounds.day = t / 86400
       WHERE t < bounds.low OR t > bounds.high
     )
     INSERT INTO DayData (TimeStamp, Serial, TotalYield, Power, PVoutput)
     SELECT empty.t, serials.Serial, (
         SELECT d.TotalYield FROM DayData d
         WHERE d.Serial = serials.Serial AND d.TimeStamp < empty.t
         ORDER BY d.TimeStamp DESC LIMIT 1
       ), 0, 0
     FROM empty CROSS JOIN (SELECT DISTINCT Serial FROM DayData) serials
     WHERE EXISTS (
       SELECT 1 FROM DayData d WHERE d.Serial = serials.Serial AND d.TimeStamp < empty.t
     )`,
    [first, last],
  );
  const after = database.query("SELECT COUNT(*) FROM DayData").values()[0][0];
  return after - before;
}

/**
 * Writes a household's consumption for every archive time: its power, and its
 * meter reading as the energy used so far.
 *
 * @param {Database} database
 * @param {number} seed
 * @returns {number} Rows written.
 */
function addConsumption(database, seed) {
  database.run("DELETE FROM Consumption");
  const times = database
    .query("SELECT DISTINCT TimeStamp FROM DayData ORDER BY TimeStamp")
    .values()
    .map(([time]) => time);
  const insert = database.prepare(
    "INSERT INTO Consumption (TimeStamp, EnergyUsed, PowerUsed) VALUES (?, ?, ?)",
  );
  let meterWh = 4_200_000;
  let previous = null;
  database.transaction(() => {
    for (const time of times) {
      const watts = householdPowerW(time, seed);
      if (previous !== null) meterWh += (watts * (time - previous)) / 3600;
      insert.run(time, Math.round(meterWh), watts);
      previous = time;
    }
  })();
  return times.length;
}

/**
 * The generation of the base system over a year: for each local day and
 * five-minute slot the power of the most recent year that has it. A
 * simulated system follows this profile, scaled to its own size. Only days
 * before today count: today is cut off at the present, and the rest of it
 * would stay dark.
 *
 * @param {Database} database
 * @param {number} now - Epoch seconds.
 * @returns {Record<string, number[]>} Watts per slot, keyed by `MM-DD`.
 */
function generationProfile(database, now) {
  const today = localTime(now).date;
  const profile = {};
  const years = {};
  for (const [time, watts] of database
    .query(
      "SELECT TimeStamp, SUM(COALESCE(Power, 0)) FROM DayData GROUP BY TimeStamp ORDER BY TimeStamp",
    )
    .values()) {
    const { date, monthDay, minuteOfDay } = localTime(time);
    if (date >= today) continue;
    const year = Number(date.slice(0, 4));
    if ((years[monthDay] ?? 0) > year) continue;
    if ((years[monthDay] ?? 0) < year) {
      years[monthDay] = year;
      profile[monthDay] = Array.from({ length: 288 }, () => 0);
    }
    profile[monthDay][Math.floor(minuteOfDay / 5)] = watts;
  }
  return profile;
}

/**
 * The largest DC power each tracker of each inverter delivered, from which
 * the installed capacity of its string is estimated.
 *
 * @param {Database} database
 * @returns {Record<string, Record<number, number>>}
 */
function trackerPeaks(database) {
  const peaks = {};
  for (const [serial, first, second] of database
    .query("SELECT Serial, MAX(Pdc1), MAX(Pdc2) FROM SpotData GROUP BY Serial")
    .values()) {
    peaks[String(serial)] = {};
    if (first > 0) peaks[String(serial)][1] = first;
    if (second > 0) peaks[String(serial)][2] = second;
  }
  return peaks;
}

/**
 * Prepares the base database in place.
 *
 * @param {string} path - Copy of the SBFspot database.
 * @param {{ now: number, seed: number }} options
 */
export function prepareBase(path, { now, seed }) {
  const database = new Database(path);
  database.run("PRAGMA journal_mode = DELETE");
  const last = database
    .query("SELECT MAX(TimeStamp) FROM DayData")
    .values()[0][0];
  const years = yearsToShift(last, now);
  shift(database, years, now);
  const nightRows = fillNights(database);
  const consumptionRows = addConsumption(database, seed);
  const [first, end] = database
    .query("SELECT MIN(TimeStamp), MAX(TimeStamp) FROM DayData")
    .values()[0];
  const profile = generationProfile(database, now);
  const peaks = trackerPeaks(database);
  const inverters = database
    .query("SELECT Serial, Type FROM Inverters ORDER BY Serial")
    .values()
    .map(([serial, type]) => ({ serial: String(serial), model: type }));
  database.run("DROP INDEX IF EXISTS demo_daydata_serial");
  database.close();
  return {
    years,
    first,
    end,
    nightRows,
    consumptionRows,
    profile,
    peaks,
    inverters,
  };
}
