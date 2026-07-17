# SBFspot push service

`pvlog-sbfspot-push` is a small, standalone Rust CLI that reads a live SBFspot
SQLite database without modifying it and uploads its telemetry to PVLog. It
aggregates all inverter rows at each `DayData.TimeStamp`, converts cumulative
yield counters into interval energy, and enriches observations with matching
`Consumption` and nearby `SpotData` values when those optional tables exist.

The uploader uses PVLog's atomic batch endpoint, stable per-timestamp
idempotency keys, bounded retries, and a cursor file that advances only after a
complete batch is accepted. Restarting it is safe. Keep the cursor when
upgrading or restarting the service; remove it only when deliberately replaying
the source database. PVLog accepts identical replays as duplicates but rejects
an idempotency key whose payload changed.

## Build and configure

Build only the uploader crate:

```sh
cargo build --release -p pvlog-sbfspot-push
```

Create a named key with only `telemetry:write` under **Account API keys**, then
put the settings in a mode-`0600` environment file such as
`/etc/pvlog/sbfspot-push.env`:

```dotenv
SBFSPOT_DATABASE=/home/pi/smadata/SBFspot.db
PVLOG_URL=https://pvlog.example
PVLOG_SYSTEM_ID=01900000-0000-7000-8000-000000000000
PVLOG_API_KEY=pvlog_REDACTED
SBFSPOT_PUSH_STATE=/var/lib/pvlog-sbfspot-push/checkpoint.json
SBFSPOT_PUSH_BATCH_SIZE=500
SBFSPOT_PUSH_POLL_INTERVAL=30
```

Do not put the API key on a command line because it may be retained in
shell history or process listings. The CLI also supports all settings as long
options; run `pvlog-sbfspot-push --help` for the full list.

Validate the database mapping without sending anything:

```sh
set -a
. /etc/pvlog/sbfspot-push.env
set +a
pvlog-sbfspot-push once --dry-run
```

Run one catch-up or remain active as a service:

```sh
pvlog-sbfspot-push once
pvlog-sbfspot-push run
```

`SBFSPOT_PUSH_START_AT` is a Unix timestamp in seconds and is honored only when
no checkpoint exists. This permits a bounded initial import without changing
the SBFspot database.

## systemd example

```ini
[Unit]
Description=Push SBFspot telemetry to PVLog
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pvlog-sbfspot
Group=pvlog-sbfspot
EnvironmentFile=/etc/pvlog/sbfspot-push.env
ExecStart=/usr/local/bin/pvlog-sbfspot-push run
Restart=on-failure
RestartSec=10s
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=/var/lib/pvlog-sbfspot-push

[Install]
WantedBy=multi-user.target
```

The service account needs read access to `SBFspot.db` and its SQLite WAL/SHM
files, write access only to the checkpoint directory, and outbound HTTPS access
to PVLog.
