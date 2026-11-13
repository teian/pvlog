#!/usr/bin/env bun
/**
 * Builds the demo data set from a real SBFspot database.
 *
 * The SBFspot history is moved by whole years so that it reaches up to now,
 * completed with nights and a household's consumption, and delivered to a
 * fresh PVLog instance by `pvlog-sbfspot-push`, the same way a real
 * installation would. A second, simulated system with a battery and a grid
 * meter gets a year of history through the ingestion API; its generation
 * follows the real days of the SBFspot system. PVOutput export files are
 * written for trying the history import.
 *
 * The result is a snapshot that `bun run demo:load` copies into `data/`
 * whenever a clean demo is wanted, and that `bun run demo:simulate` keeps
 * live. Nothing is uploaded anywhere but to the local instance started here.
 *
 * Usage:
 *   bun scripts/demo/build.mjs --sbfspot /path/to/SBFspot.db [--out demo-data]
 *     [--live-days 365] [--latitude 51.0] [--longitude 10.0] [--port 18197]
 */

import { Database } from "bun:sqlite";
import { cpSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

import { deliverLive } from "./lib/liveDelivery.mjs";
import { initialState, LIVE_SYSTEM } from "./lib/liveSystem.mjs";
import { localTime, TIMEZONE } from "./lib/localTime.mjs";
import { signIn } from "./lib/pvlogClient.mjs";
import { writePvoutputFiles } from "./lib/pvoutputFiles.mjs";
import { copyDatabase, prepareBase } from "./lib/sbfspotBase.mjs";

const ROOT = path.resolve(import.meta.dir, "../..");
export const DEMO_USER = {
  email: "demo@pvlog.local",
  password: "demo-password",
  displayName: "Demo",
};
const HOUSEHOLD_SEED = 1;

function options(argv) {
  const values = {
    sbfspot: process.env.PVLOG_DEMO_SBFSPOT_DB,
    out: "demo-data",
    liveDays: 365,
    latitude: 51.0,
    longitude: 10.0,
    port: 18197,
  };
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index]
      .replace(/^--/, "")
      .replace(/-(\w)/g, (_, c) => c.toUpperCase());
    if (!(key in values)) throw new Error(`unknown option ${argv[index]}`);
    values[key] =
      typeof values[key] === "number"
        ? Number(argv[index + 1])
        : argv[index + 1];
  }
  if (!values.sbfspot)
    throw new Error(
      "name the SBFspot database with --sbfspot or PVLOG_DEMO_SBFSPOT_DB",
    );
  return values;
}

const step = (text) => console.log(`\n▶ ${text}`);

/** Runs a command and fails when it fails. */
async function run(command, env = {}) {
  const child = Bun.spawn(command, {
    cwd: ROOT,
    env: { ...process.env, ...env },
    stdout: "inherit",
    stderr: "inherit",
  });
  if ((await child.exited) !== 0)
    throw new Error(`${command.join(" ")} failed`);
}

/** Starts a process and returns it with its collected output. */
function start(command, env) {
  const child = Bun.spawn(command, {
    cwd: ROOT,
    env: { ...process.env, ...env },
    stdout: "pipe",
    stderr: "pipe",
  });
  const output = { text: "" };
  for (const stream of [child.stdout, child.stderr])
    (async () => {
      for await (const chunk of stream)
        output.text += new TextDecoder().decode(chunk);
    })();
  return { child, output };
}

async function waitFor(check, what, seconds = 120) {
  for (let elapsed = 0; elapsed < seconds * 2; elapsed += 1) {
    if (await check().catch(() => false)) return;
    await Bun.sleep(500);
  }
  throw new Error(`timed out waiting for ${what}`);
}

/**
 * Creates the demo user directly in the management database: there is no
 * public way to create the first account. Signing in provisions the user's
 * account; the user also administers the instance.
 */
async function createUser(managementPath) {
  const database = new Database(managementPath);
  const now = Date.now();
  const id = Bun.randomUUIDv7("buffer");
  const hash = await Bun.password.hash(DEMO_USER.password, {
    algorithm: "argon2id",
    memoryCost: 19_456,
    timeCost: 2,
  });
  database.transaction(() => {
    database.run(
      "INSERT INTO users (id, email, display_name, status, email_verified_at, created_at, updated_at) VALUES (?, ?, ?, 'active', ?, ?, ?)",
      [id, DEMO_USER.email, DEMO_USER.displayName, now, now, now],
    );
    database.run(
      "INSERT INTO local_credentials (user_id, password_hash, password_changed_at) VALUES (?, ?, ?)",
      [id, hash, now],
    );
    let role = database
      .query(
        "SELECT id FROM rbac_roles WHERE built_in_key = 'instance_administrator' AND account_id IS NULL",
      )
      .values()[0]?.[0];
    if (!role) {
      role = Bun.randomUUIDv7("buffer");
      database.run(
        "INSERT INTO rbac_roles (id, account_id, name, role_kind, built_in_key, created_by, created_at, updated_at) VALUES (?, NULL, 'instance_administrator', 'built_in', 'instance_administrator', ?, ?, ?)",
        [role, id, now, now],
      );
    }
    database.run(
      "INSERT INTO rbac_role_assignments (id, role_id, principal_type, principal_id, scope_type, delegated_by, created_at) VALUES (?, ?, 'user', ?, 'instance', ?, ?)",
      [Bun.randomUUIDv7("buffer"), role, id, id, now],
    );
  })();
  database.close();
}

/** Capacity of a string estimated from the largest DC power its tracker delivered. */
const capacityOf = (peakW) => Math.max(500, Math.ceil(peakW / 0.85 / 50) * 50);

/** Adopts every proposed device with one string per tracker. */
async function adoptProposals(session, systemId, strings) {
  const proposals = await session.request(
    "GET",
    `/api/v1/systems/${systemId}/discovery-proposals`,
  );
  for (const proposal of proposals) {
    const body = {
      strings: proposal.trackers.map((tracker) =>
        strings(proposal.deviceSerial, tracker),
      ),
    };
    if (body.strings.length === 0) continue;
    await session.request(
      "POST",
      `/api/v1/systems/${systemId}/discovery-proposals/${proposal.id}/adopt`,
      body,
    );
    console.log(
      `  adopted ${proposal.deviceSerial} (${proposal.model ?? "?"}) with ${body.strings.length} string(s)`,
    );
  }
}

/** Removes the write-ahead logs so that the files can be copied as they are. */
function checkpoint(directory) {
  const files = [path.join(directory, "management.sqlite3")];
  const accounts = path.join(directory, "accounts");
  for (const name of new Bun.Glob("*.sqlite3").scanSync(accounts))
    files.push(path.join(accounts, name));
  for (const file of files) {
    const database = new Database(file);
    database.run("PRAGMA wal_checkpoint(TRUNCATE)");
    database.close();
  }
}

async function main() {
  const settings = options(process.argv.slice(2));
  const out = path.resolve(ROOT, settings.out);
  const work = path.join(out, ".build");
  const instance = path.join(work, "instance");
  rmSync(work, { recursive: true, force: true });
  mkdirSync(path.join(instance, "accounts"), { recursive: true });
  const now = Math.floor(Date.now() / 1000);
  const baseUrl = `http://127.0.0.1:${settings.port}`;

  step("Preparing the SBFspot history");
  const basePath = path.join(work, "SBFspot.db");
  copyDatabase(settings.sbfspot, basePath);
  const base = prepareBase(basePath, { now, seed: HOUSEHOLD_SEED });
  console.log(
    `  moved by ${base.years} year(s); ${new Date(base.first * 1000).toISOString()} – ${new Date(base.end * 1000).toISOString()}`,
  );
  console.log(
    `  ${base.nightRows} night rows, ${base.consumptionRows} consumption rows`,
  );

  step("Building the binaries");
  await run([
    "cargo",
    "build",
    "--release",
    "-p",
    "pvlog",
    "-p",
    "pvlog-sbfspot-push",
  ]);

  const env = {
    PVLOG_DATABASE__BACKEND: "sqlite",
    PVLOG_DATABASE__SQLITE__MANAGEMENT_PATH: path.join(
      instance,
      "management.sqlite3",
    ),
    PVLOG_DATABASE__SQLITE__ACCOUNTS_DIR: path.join(instance, "accounts"),
    PVLOG_HTTP__BIND: `127.0.0.1:${settings.port}`,
    RUST_LOG: "warn,pvlog=info",
  };
  step("Creating a fresh instance");
  await run(["target/release/pvlog", "migrate", "apply"], env);
  await createUser(env.PVLOG_DATABASE__SQLITE__MANAGEMENT_PATH);
  const server = start(["target/release/pvlog", "server"], env);
  try {
    await waitFor(
      async () => (await fetch(`${baseUrl}/api/v1/session`)).ok,
      "the server",
    );
    const session = await signIn(baseUrl, DEMO_USER.email, DEMO_USER.password);
    const location = {
      timezone: TIMEZONE,
      countryCode: "DE",
      locationLabel: "Demo",
      latitudeMicrodegrees: Math.round(settings.latitude * 1e6),
      longitudeMicrodegrees: Math.round(settings.longitude * 1e6),
    };

    step("Delivering the SBFspot system");
    const sbfspotSystem = await session.request("POST", "/api/v1/systems", {
      name: "Demo SBFspot",
      description:
        "Real SBFspot history, moved to the present, with a simulated household",
      commissioningDate: localTime(base.first).date,
      statusIntervalSeconds: 300,
      ...location,
    });
    const sbfspotSource = await session.request(
      "POST",
      `/api/v1/systems/${sbfspotSystem.id}/data-sources`,
      { name: "SBFspot", profile: "sbfspot" },
    );
    const started = Date.now();
    await run(["target/release/pvlog-sbfspot-push", "once"], {
      SBFSPOT_DATABASE: basePath,
      PVLOG_URL: baseUrl,
      PVLOG_SYSTEM_ID: sbfspotSystem.id,
      PVLOG_API_KEY: sbfspotSource.configuration.apiKey,
      SBFSPOT_PUSH_STATE: path.join(work, "sbfspot-push.json"),
      SBFSPOT_PUSH_START_AT: "0",
      SBFSPOT_PUSH_BATCH_SIZE: "1000",
    });
    console.log(
      `  delivered in ${Math.round((Date.now() - started) / 1000)} s`,
    );
    await adoptProposals(session, sbfspotSystem.id, (serial, tracker) => ({
      tracker,
      name: `Tracker ${tracker}`,
      capacityWatts: capacityOf(base.peaks[serial]?.[tracker] ?? 1000),
    }));
    await session.request(
      "POST",
      `/api/v1/systems/${sbfspotSystem.id}/data-sources`,
      {
        name: "PVOutput-Import",
        profile: "import",
      },
    );

    step("Delivering the simulated system");
    const baseCapacity = Object.values(base.peaks)
      .flatMap((trackers) => Object.values(trackers))
      .reduce((sum, peak) => sum + capacityOf(peak), 0);
    const liveCapacity = LIVE_SYSTEM.trackers.reduce(
      (sum, tracker) => sum + tracker.capacityW,
      0,
    );
    const liveBase = {
      profile: base.profile,
      scale: liveCapacity / baseCapacity,
    };
    const liveSystem = await session.request("POST", "/api/v1/systems", {
      name: LIVE_SYSTEM.name,
      description:
        "Simulated hybrid inverter with battery; bun run demo:simulate keeps it live",
      statusIntervalSeconds: 60,
      ...location,
    });
    const liveSource = await session.request(
      "POST",
      `/api/v1/systems/${liveSystem.id}/data-sources`,
      { name: "smalog (simuliert)", profile: "smalog" },
    );
    const target = {
      baseUrl,
      systemId: liveSystem.id,
      apiKey: liveSource.configuration.apiKey,
    };
    const state = initialState(now - settings.liveDays * 86_400);
    // History at five minutes, the last day at the live cadence of a minute.
    let delivered = await deliverLive(target, state, liveBase, {
      until: now - 86_400,
      stepSeconds: 300,
    });
    delivered += await deliverLive(target, state, liveBase, {
      until: now,
      stepSeconds: 60,
    });
    console.log(`  ${delivered} measurements`);
    await adoptProposals(session, liveSystem.id, (_, tracker) => {
      const string = LIVE_SYSTEM.trackers.find(
        (candidate) => candidate.number === tracker,
      );
      return {
        tracker,
        name: string?.name ?? `Tracker ${tracker}`,
        capacityWatts: string?.capacityW ?? 1000,
        orientationDegrees: string?.orientation ?? null,
        tiltDegrees: string?.tilt ?? null,
      };
    });

    step("Aggregating");
    const aggregationStarted = Date.now();
    const worker = start(
      ["target/release/pvlog", "worker", "--interval-seconds", "2"],
      {
        ...env,
        RUST_LOG: "warn,pvlog=debug",
      },
    );
    try {
      await waitFor(
        async () =>
          worker.output.text.includes(
            "aggregation cycle found no pending observations",
          ),
        "the aggregation",
        3600,
      );
      console.log(
        `  aggregated in ${Math.round((Date.now() - aggregationStarted) / 1000)} s`,
      );
    } finally {
      worker.child.kill("SIGINT");
      await worker.child.exited;
    }

    step("Writing the snapshot");
    const pvoutput = writePvoutputFiles(path.join(out, "pvoutput"), {
      profile: base.profile,
      firstDate: localTime(base.first).date,
      seed: HOUSEHOLD_SEED,
    });
    const demo = {
      builtAt: new Date(now * 1000).toISOString(),
      movedByYears: base.years,
      user: DEMO_USER,
      systems: {
        sbfspot: { id: sbfspotSystem.id, name: sbfspotSystem.name },
        live: {
          id: liveSystem.id,
          name: liveSystem.name,
          dataSourceId: liveSource.dataSource.id,
          apiKey: liveSource.configuration.apiKey,
        },
      },
      liveBaseScale: liveBase.scale,
      pvoutputFiles: pvoutput,
    };
    server.child.kill("SIGINT");
    await server.child.exited;
    checkpoint(instance);
    const snapshot = path.join(out, "snapshot");
    rmSync(snapshot, { recursive: true, force: true });
    mkdirSync(path.join(snapshot, "demo"), { recursive: true });
    cpSync(
      path.join(instance, "management.sqlite3"),
      path.join(snapshot, "management.sqlite3"),
    );
    cpSync(path.join(instance, "accounts"), path.join(snapshot, "accounts"), {
      recursive: true,
    });
    writeFileSync(
      path.join(snapshot, "demo", "demo.json"),
      `${JSON.stringify(demo, null, 2)}\n`,
    );
    writeFileSync(
      path.join(snapshot, "demo", "live-state.json"),
      `${JSON.stringify(state)}\n`,
    );
    writeFileSync(
      path.join(snapshot, "demo", "profile.json"),
      JSON.stringify(base.profile),
    );
    rmSync(work, { recursive: true, force: true });
    console.log(`\n✔ Demo data written to ${path.relative(ROOT, snapshot)}`);
    console.log(`  Load it with: bun run demo:load`);
    console.log(`  Sign in as ${DEMO_USER.email} / ${DEMO_USER.password}`);
  } finally {
    server.child.kill("SIGINT");
  }
}

if (import.meta.main) await main();
