# SBFspot push service

`pvlog-sbfspot-push` is a small, standalone Rust CLI that reads a live SBFspot
SQLite database without modifying it and uploads its telemetry to PVLog. For
every archive time in `DayData` it sends one reading: the totals of the system,
and for each inverter serial the values SBFspot recorded for it. It also
uploads the inverters' identity and their events.

The uploader uses PVLog's atomic batch endpoint, stable per-timestamp
idempotency keys, bounded retries, and a cursor file that advances only after a
complete batch is accepted. Restarting it is safe. Keep the cursor when
upgrading or restarting the service; remove it only when deliberately replaying
the source database. PVLog accepts identical replays as duplicates but rejects
an idempotency key whose payload changed, so a range that an earlier version
of the uploader delivered cannot be replayed with the per-inverter values
added.

## What is uploaded

The system's totals travel in the first-class fields of a reading, so
dashboards work without any setup. The values of each inverter travel as
measurement channels `inv.<serial>.<measurement>`, the same keys
[smalog](smalog-export.md) uses, in the integer units of those keys.

| SBFspot                                          | PVLog                                                 |
| ------------------------------------------------ | ----------------------------------------------------- |
| `DayData.TimeStamp`                              | Time of the reading                                   |
| `DayData.Power`, summed over serials             | `generationPowerWatts`                                |
| `DayData.TotalYield`, growth per serial          | `generationEnergyWh`                                  |
| `DayData.TotalYield`                             | `inv.<serial>.energy.lifetime` (Wh)                   |
| `Consumption.PowerUsed`                          | `consumptionPowerWatts`                               |
| `Consumption.EnergyUsed`, growth                 | `consumptionEnergyWh`                                 |
| `SpotData.Pac1`…`Pac3`                           | `inv.<serial>.ac.l1.power`…`l3.power` (W)             |
| `SpotData.Iac1`…`Iac3` (A)                       | `inv.<serial>.ac.l1.current`…`l3.current` (mA)        |
| `SpotData.Uac1`…`Uac3` (V)                       | `inv.<serial>.ac.l1.voltage`…`l3.voltage` (mV)        |
| `SpotData.Pdc1`, `Pdc2`                          | `inv.<serial>.mppt.1.power`, `mppt.2.power` (W)       |
| `SpotData.Idc1`, `Idc2` (A)                      | `inv.<serial>.mppt.1.current`, `mppt.2.current` (mA)  |
| `SpotData.Udc1`, `Udc2` (V)                      | `inv.<serial>.mppt.1.voltage`, `mppt.2.voltage` (mV)  |
| `SpotData.Frequency` (Hz)                        | `inv.<serial>.grid.frequency` (mHz)                   |
| `SpotData.EToday`                                | `inv.<serial>.energy.today` (Wh)                      |
| `SpotData.OperatingTime` (h)                     | `inv.<serial>.operating_time` (s)                     |
| `SpotData.FeedInTime` (h)                        | `inv.<serial>.feed_in_time` (s)                       |
| `SpotData.Status`                                | `inv.<serial>.status` (SMA code)                      |
| `SpotData.GridRelay`                             | `inv.<serial>.grid_relay` (SMA code)                  |
| `SpotData.Temperature` (°C)                      | `inv.<serial>.temperature` (m°C)                      |
| `SpotData.BT_Signal` (%)                         | `inv.<serial>.signal` (‰)                             |
| `SpotData.Uac1`, `Temperature`, averaged         | `voltageMillivolts`, `temperatureMillidegreesCelsius` |
| `Inverters.Serial`, `Name`, `Type`, `SW_Version` | `devices`: identifier, reported name, model, firmware |
| `EventData`                                      | Device events, identified by serial and `EntryID`     |

A reading takes the `SpotData` row of each serial that is closest to its
archive time, within 150 seconds. `SpotData.ETotal` is not sent; the lifetime
counter comes from `DayData`, which SBFspot also fills from the inverter's
archive.

SBFspot writes zeros for a phase or DC input an inverter does not have. A phase
or input whose power, current and voltage are all zero is therefore left out,
and PVLog stores nothing for it instead of a zero. `Status` and `GridRelay` are
stored by SBFspot as text; `OK`, `Warning`, `Fault`, `Off`, `Closed`, `Open`
and `N/A` become the SMA codes 307, 455, 35, 303, 51, 311 and 16777213. Any
other text is left out.

Interval energy is what each inverter's `TotalYield` grew by. It is left out
for a reading whose inverters are not the inverters of the reading before:
when a serial is silent, when a new serial appears, and when a counter went
back. A replaced inverter therefore creates no energy that nobody generated.
Its new serial arrives under new channel keys, and PVLog proposes it as an
inverter in the system's settings.

`Consumption`, `SpotData`, `Inverters` and `EventData` are optional. When a
table is absent the uploader delivers what exists; the summary of a pass and
the service log name the groups that could not be delivered
(`consumption`, `spot_values`, `inverter_identity`, `events`).

## Build and configure

Build only the uploader crate:

```sh
cargo build --release -p pvlog-sbfspot-push
```

Add a data source with the **SBFspot** profile under the system's **Settings →
Data sources** (see [Setting up a system](system-setup.md)). PVLog shows the
environment file below with the server URL, the system identifier and a key
that can only upload for this system. Save it with mode `0600`, for example as
`/etc/pvlog/sbfspot-push.env`, and add the settings of the local database:

```dotenv
SBFSPOT_DATABASE=/home/pi/smadata/SBFspot.db
PVLOG_URL=https://pvlog.example
PVLOG_SYSTEM_ID=01900000-0000-7000-8000-000000000000
PVLOG_API_KEY=pvlog_REDACTED
SBFSPOT_PUSH_START_AT=0
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

The summary printed by `once` counts the readings and events of the pass and
lists the groups of values the database does not hold:

```json
{
  "batches": 1,
  "observations": 288,
  "lastTimestamp": 1800000000,
  "events": 3,
  "unavailable": ["consumption"]
}
```

## Backfilling history

The first pass uploads everything the database holds after
`SBFSPOT_PUSH_START_AT`, a Unix timestamp in seconds, and every later pass
continues from the cursor file. The generated configuration sets the start to
`0`, which uploads the whole database. Set it to a later time to bound the
initial import; it is honored only while no cursor file exists. Events from
before the start are not uploaded either.

A backfill needs no other tool and does not change the SBFspot database. Years
of history take a number of batches; the uploader retries a batch that PVLog
asks to send later and resumes after an interruption where it stopped.

## Moving from SBFspot to smalog

[smalog](smalog-export.md) records more than SBFspot and delivers it through
PVLog's [ingestion API](ingestion-api.md). Both use the same channel keys, so
the measurement channels of a system, their bindings to inverters and strings
and their history stay in place when the logger changes.

1. Import the SBFspot database into smalog with smalog's own migration, so
   that smalog continues the history.
2. Stop `pvlog-sbfspot-push` and note the `lastTimestamp` of its cursor file.
3. Add a data source with the **smalog** profile to the same system and paste
   the generated section into smalog's configuration.
4. Set `start_at` in that section to the day after the last upload of
   `pvlog-sbfspot-push`. Readings of two data sources for the same time are
   two readings, so history that SBFspot already delivered must not be
   delivered by smalog again.
5. Delete the SBFspot data source once smalog is receiving. Its readings stay.

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
