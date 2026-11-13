#!/usr/bin/env bun
/**
 * Replaces the local development data in `data/` with the demo snapshot that
 * `bun run demo:build` wrote. Data that did not come from the demo is moved
 * aside to `data.bak-<time>` first; a previous demo is replaced. Stop the
 * server before loading: it keeps the databases open.
 *
 * Usage: bun scripts/demo/load.mjs [--snapshot demo-data/snapshot] [--data data]
 *   [--server http://127.0.0.1:18087]
 */

import { cpSync, existsSync, readFileSync, renameSync, rmSync } from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dir, "../..");

function options(argv) {
  const values = {
    snapshot: "demo-data/snapshot",
    data: "data",
    server: "http://127.0.0.1:18087",
  };
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index].replace(/^--/, "");
    if (!(key in values)) throw new Error(`unknown option ${argv[index]}`);
    values[key] = argv[index + 1];
  }
  return values;
}

async function serverRuns(url) {
  try {
    await fetch(`${url}/api/v1/session`, { signal: AbortSignal.timeout(1000) });
    return true;
  } catch {
    return false;
  }
}

const settings = options(process.argv.slice(2));
const snapshot = path.resolve(ROOT, settings.snapshot);
const data = path.resolve(ROOT, settings.data);
if (!existsSync(path.join(snapshot, "demo", "demo.json"))) {
  console.error(
    `No demo snapshot in ${settings.snapshot}; run bun run demo:build first.`,
  );
  process.exit(1);
}
if (await serverRuns(settings.server)) {
  console.error(
    `A server answers at ${settings.server}. Stop it before loading the demo data.`,
  );
  process.exit(1);
}
if (existsSync(data)) {
  if (existsSync(path.join(data, "demo", "demo.json"))) {
    rmSync(data, { recursive: true, force: true });
  } else {
    const aside = `${data}.bak-${new Date().toISOString().replace(/[:.]/g, "-")}`;
    renameSync(data, aside);
    console.log(
      `Moved the previous data aside to ${path.relative(ROOT, aside)}`,
    );
  }
}
cpSync(snapshot, data, { recursive: true });
const demo = JSON.parse(
  readFileSync(path.join(data, "demo", "demo.json"), "utf8"),
);
console.log(`Loaded the demo data built at ${demo.builtAt}.`);
console.log(`Sign in as ${demo.user.email} / ${demo.user.password}`);
console.log(
  "Start the server, then keep the live system running with: bun run demo:simulate",
);
