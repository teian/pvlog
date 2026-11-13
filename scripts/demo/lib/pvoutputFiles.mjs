/**
 * PVOutput export files for the base system, to try the history import with:
 * a year of daily output before the SBFspot history starts, status files of
 * single days before it, and one status day that overlaps it, so that the
 * dry run reports the overlap and the commit skips it.
 */

import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

import { householdPowerW } from "./household.mjs";
import { localTime } from "./localTime.mjs";

const DAY = 86_400;

/** Epoch seconds of local midnight of a date in the demo timezone (good to the hour). */
function localMidnight(date) {
  const utc = Date.parse(`${date}T00:00:00Z`) / 1000;
  const { minuteOfDay } = localTime(utc);
  // Berlin is one or two hours ahead of UTC.
  return (
    utc - (minuteOfDay >= 12 * 60 ? minuteOfDay - 24 * 60 : minuteOfDay) * 60
  );
}

function addDays(date, days) {
  return new Date(Date.parse(`${date}T12:00:00Z`) + days * DAY * 1000)
    .toISOString()
    .slice(0, 10);
}

/** Generation and household of one local day, slot by slot. */
function daySlots(profile, date, seed) {
  const generation = profile[date.slice(5)] ?? profile["02-28"];
  const midnight = localMidnight(date);
  return generation.map((watts, slot) => ({
    at: midnight + slot * 300,
    generationW: watts,
    consumptionW: householdPowerW(midnight + slot * 300, seed),
  }));
}

const condition = (ratio) =>
  ratio > 0.8
    ? "Fine"
    : ratio > 0.55
      ? "Partly Cloudy"
      : ratio > 0.3
        ? "Mostly Cloudy"
        : ratio > 0.15
          ? "Cloudy"
          : "Showers";

const kwh = (wh) => (wh / 1000).toFixed(3);

/** One row per day of the output export. */
function outputFile(profile, first, last, seed) {
  const best = Math.max(
    ...Object.values(profile).map((day) =>
      day.reduce((sum, watts) => sum + watts, 0),
    ),
  );
  const rows = [
    "Date,Generated (kWh),Efficiency,Exported (kWh),Peak Power (kW),Peak Time,Conditions,Min Temp,Max Temp,Consumption (kWh),Peak Energy Import (kWh),Off Peak Energy Import (kWh),Shoulder Energy Import (kWh),High Shoulder Energy Import (kWh),Comments",
  ];
  for (let date = last; date >= first; date = addDays(date, -1)) {
    const slots = daySlots(profile, date, seed);
    let generated = 0;
    let exported = 0;
    let consumed = 0;
    let imported = 0;
    let peak = { watts: 0, slot: 0 };
    slots.forEach((slot, index) => {
      generated += slot.generationW / 12;
      consumed += slot.consumptionW / 12;
      exported += Math.max(0, slot.generationW - slot.consumptionW) / 12;
      imported += Math.max(0, slot.consumptionW - slot.generationW) / 12;
      if (slot.generationW > peak.watts)
        peak = { watts: slot.generationW, slot: index };
    });
    const month = Number(date.slice(5, 7));
    const warmth = -Math.cos(((month - 1) / 12) * 2 * Math.PI);
    const minimum = (4 + 8 * warmth).toFixed(1);
    const maximum = (11 + 11 * warmth).toFixed(1);
    const peakTime = `${String(Math.floor(peak.slot / 12)).padStart(2, "0")}:${String((peak.slot % 12) * 5).padStart(2, "0")}`;
    rows.push(
      [
        date,
        (generated / 1000).toFixed(2),
        "",
        (exported / 1000).toFixed(2),
        (peak.watts / 1000).toFixed(2),
        peakTime,
        condition(generated / best),
        minimum,
        maximum,
        (consumed / 1000).toFixed(2),
        ((imported * 0.4) / 1000).toFixed(2),
        ((imported * 0.6) / 1000).toFixed(2),
        "",
        "",
        "",
      ].join(","),
    );
  }
  return `${rows.join("\n")}\n`;
}

/** Five-minute statuses of one day, with the energy since midnight. */
function statusFile(profile, date, seed) {
  const rows = [
    "Date,Time,Energy Generated (kWh),Efficiency,Power (W),Average,Normalised,Energy Used (kWh),Power Used (W),Temperature,Voltage",
  ];
  let generated = 0;
  let used = 0;
  daySlots(profile, date, seed).forEach((slot, index) => {
    generated += slot.generationW / 12;
    used += slot.consumptionW / 12;
    const time = `${String(Math.floor(index / 12)).padStart(2, "0")}:${String((index % 12) * 5).padStart(2, "0")}`;
    rows.push(
      [
        date,
        time,
        kwh(generated),
        "",
        Math.round(slot.generationW),
        "",
        "",
        kwh(used),
        slot.consumptionW,
        (8 + Math.sin((index / 288) * Math.PI) * 9).toFixed(1),
        (229 + (index % 7) * 0.4).toFixed(1),
      ].join(","),
    );
  });
  return `${rows.join("\n")}\n`;
}

/**
 * Writes the files and returns their names.
 *
 * @param {string} directory
 * @param {{ profile: Record<string, number[]>, firstDate: string, seed: number }} base
 * @returns {string[]}
 */
export function writePvoutputFiles(directory, { profile, firstDate, seed }) {
  mkdirSync(directory, { recursive: true });
  const files = {};
  const lastBefore = addDays(firstDate, -1);
  files[`output-${addDays(firstDate, -365)}-to-${lastBefore}.csv`] = outputFile(
    profile,
    addDays(firstDate, -365),
    lastBefore,
    seed,
  );
  for (const offset of [-3, -2, -1])
    files[`status-${addDays(firstDate, offset)}.csv`] = statusFile(
      profile,
      addDays(firstDate, offset),
      seed,
    );
  // A day the SBFspot history covers too: its rows overlap and are skipped.
  const overlap = addDays(firstDate, 1);
  files[`status-${overlap}-overlaps-sbfspot.csv`] = statusFile(
    profile,
    overlap,
    seed,
  );
  for (const [name, content] of Object.entries(files))
    writeFileSync(path.join(directory, name), content);
  return Object.keys(files);
}
