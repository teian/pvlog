#!/usr/bin/env bun
/**
 * Keeps the simulated demo system live: it first delivers everything since
 * the last simulated instant, then one measurement per interval, as a logger
 * would. The simulator's position is kept in `data/demo/live-state.json`, so
 * that it continues where it stopped; loading the demo again resets it.
 *
 * Usage: bun scripts/demo/simulate.mjs [--server http://127.0.0.1:18087]
 *   [--interval 60] [--data data] [--once]
 */

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

import { deliverLive } from "./lib/liveDelivery.mjs";

const ROOT = path.resolve(import.meta.dir, "../..");

function options(argv) {
  const values = {
    server: "http://127.0.0.1:18087",
    interval: 60,
    data: "data",
    once: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index].replace(/^--/, "");
    if (!(key in values)) throw new Error(`unknown option ${argv[index]}`);
    if (key === "once") values.once = true;
    else {
      values[key] =
        typeof values[key] === "number"
          ? Number(argv[index + 1])
          : argv[index + 1];
      index += 1;
    }
  }
  return values;
}

const settings = options(process.argv.slice(2));
const directory = path.resolve(ROOT, settings.data, "demo");
const read = (name) =>
  JSON.parse(readFileSync(path.join(directory, name), "utf8"));
let demo;
try {
  demo = read("demo.json");
} catch {
  console.error(
    `No demo data in ${settings.data}/; run bun run demo:load first.`,
  );
  process.exit(1);
}
const statePath = path.join(directory, "live-state.json");
const state = read("live-state.json");
const base = { profile: read("profile.json"), scale: demo.liveBaseScale };
const target = {
  baseUrl: settings.server,
  systemId: demo.systems.live.id,
  apiKey: demo.systems.live.apiKey,
};
const save = (current) =>
  writeFileSync(statePath, `${JSON.stringify(current)}\n`);

async function deliverUntilNow() {
  const delivered = await deliverLive(target, state, base, {
    until: Math.floor(Date.now() / 1000),
    stepSeconds: settings.interval,
    onBatch: save,
  });
  const at = new Date(state.lastAt * 1000).toLocaleString("de-DE");
  const soc = Math.round((state.batteryChargeWh / 10_000) * 100);
  console.log(`${at}: ${delivered} measurement(s) delivered, battery ${soc} %`);
}

console.log(`Simulating "${demo.systems.live.name}" at ${settings.server}`);
await deliverUntilNow();
if (!settings.once) {
  let stopping = false;
  process.on("SIGINT", () => {
    stopping = true;
  });
  while (!stopping) {
    const wait =
      settings.interval * 1000 -
      (Date.now() % (settings.interval * 1000)) +
      500;
    await Bun.sleep(wait);
    if (stopping) break;
    try {
      await deliverUntilNow();
    } catch (error) {
      console.error(
        `Delivery failed, retrying next interval: ${error.message}`,
      );
    }
  }
}
