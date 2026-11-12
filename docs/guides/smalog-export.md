# smalog export

[smalog](https://github.com/teian/smalog) is an SMA logger that keeps its own
database. Its PVLog export reads that database and delivers every plant value
to one PVLog system through the [ingestion API](ingestion-api.md). This page
maps each column of smalog's schema version 1 to the member it is sent as and
to where PVLog keeps it.

## Connecting smalog

Create a data source with the profile `smalog` in the system's settings. PVLog
shows the export section for smalog's `config.toml` once, with the server and
the system filled in, and the environment line that carries the key:

```toml
[pvlog]
enabled = true
url = "https://pvlog.example"
system_id = "0190a1b2-0000-7000-8000-000000000001"
api_key = "${PVLOG_API_KEY}"
```

```sh
PVLOG_API_KEY=<the key shown once>
```

The key is written only into the environment, so `config.toml` can be shared.

## How rows become a request

The export serializes rows field for field. It computes no sums, differences
or unit conversions; smalog's integer units are the units of the ingestion API.
A column that is `NULL` and a child row that does not exist are left out, and
PVLog stores the value as missing.

| smalog rows                                                                                                       | Request list     | Identity in PVLog          |
| ----------------------------------------------------------------------------------------------------------------- | ---------------- | -------------------------- |
| `inverters`                                                                                                       | `devices`        | Serial number              |
| `inverter_measurements`, `mppt_measurements`, `battery_measurements`, `site_consumption_measurements` of one time | `measurements`   | Data source and time       |
| `inverter_energy_samples`                                                                                         | `energySamples`  | Serial number and time     |
| `inverter_daily_yields`                                                                                           | `dailySummaries` | Serial number and date     |
| `inverter_events`                                                                                                 | `events`         | Serial number and event id |

The device identifier is the serial number written as text. PVLog stores each
device value as a measurement channel `inv.<serial>.<measurement>`.

## Column mapping

Columns that are surrogate keys or the logger's own bookkeeping are not sent;
the table says what takes their place.

### `inverters`

| Column                       | Member of `devices[]` | Kept in PVLog as                                   |
| ---------------------------- | --------------------- | -------------------------------------------------- |
| `inverters.inverter_id`      | not sent              | Surrogate key; the serial number identifies        |
| `inverters.serial_number`    | `id`                  | Device identity; serial of the matching inverter   |
| `inverters.susy_id`          | `familyId`            | Device identity                                    |
| `inverters.configured_name`  | `configuredName`      | Device identity; name of a discovery proposal      |
| `inverters.device_name`      | `reportedName`        | Device identity; name of a discovery proposal      |
| `inverters.model`            | `model`               | Device identity; model of the matching inverter    |
| `inverters.firmware_version` | `firmware`            | Device identity; firmware of the matching inverter |
| `inverters.transport`        | `transport`           | Device identity                                    |
| `inverters.first_seen_at`    | not sent              | PVLog records when it first received the device    |
| `inverters.last_seen_at`     | not sent              | PVLog records when it last received the device     |

### `inverter_measurements`

| Column                                            | Member of `measurements[].devices[]` | Channel `inv.<serial>.…`  |
| ------------------------------------------------- | ------------------------------------ | ------------------------- |
| `inverter_measurements.measurement_id`            | not sent                             | Surrogate key             |
| `inverter_measurements.inverter_id`               | `id` (the serial number)             | Serial in the channel key |
| `inverter_measurements.measured_at`               | `measurements[].measuredAt`          | Time of the reading       |
| `inverter_measurements.ac_power_l1_w`             | `ac[phase=1].powerW`                 | `ac.l1.power` (W)         |
| `inverter_measurements.ac_power_l2_w`             | `ac[phase=2].powerW`                 | `ac.l2.power` (W)         |
| `inverter_measurements.ac_power_l3_w`             | `ac[phase=3].powerW`                 | `ac.l3.power` (W)         |
| `inverter_measurements.ac_current_l1_ma`          | `ac[phase=1].currentMa`              | `ac.l1.current` (mA)      |
| `inverter_measurements.ac_current_l2_ma`          | `ac[phase=2].currentMa`              | `ac.l2.current` (mA)      |
| `inverter_measurements.ac_current_l3_ma`          | `ac[phase=3].currentMa`              | `ac.l3.current` (mA)      |
| `inverter_measurements.ac_voltage_l1_mv`          | `ac[phase=1].voltageMv`              | `ac.l1.voltage` (mV)      |
| `inverter_measurements.ac_voltage_l2_mv`          | `ac[phase=2].voltageMv`              | `ac.l2.voltage` (mV)      |
| `inverter_measurements.ac_voltage_l3_mv`          | `ac[phase=3].voltageMv`              | `ac.l3.voltage` (mV)      |
| `inverter_measurements.grid_frequency_mhz`        | `gridFrequencyMhz`                   | `grid.frequency` (mHz)    |
| `inverter_measurements.grid_import_power_w`       | `gridImportPowerW`                   | `grid.import_power` (W)   |
| `inverter_measurements.grid_export_power_w`       | `gridExportPowerW`                   | `grid.export_power` (W)   |
| `inverter_measurements.energy_today_wh`           | `energyTodayWh`                      | `energy.today` (Wh)       |
| `inverter_measurements.energy_total_wh`           | `energyLifetimeWh`                   | `energy.lifetime` (Wh)    |
| `inverter_measurements.operating_time_s`          | `operatingTimeS`                     | `operating_time` (s)      |
| `inverter_measurements.feed_in_time_s`            | `feedInTimeS`                        | `feed_in_time` (s)        |
| `inverter_measurements.device_status_code`        | `statusCode`                         | `status` (code)           |
| `inverter_measurements.grid_relay_status_code`    | `gridRelayStatusCode`                | `grid_relay` (code)       |
| `inverter_measurements.temperature_millicelsius`  | `temperatureMillicelsius`            | `temperature` (m°C)       |
| `inverter_measurements.bluetooth_signal_permille` | `signalPermille`                     | `signal` (‰)              |

From these PVLog derives the system's generation power, lifetime and interval
energy and its grid import and export; see
[what PVLog derives](ingestion-api.md#what-pvlog-derives).

### `mppt_measurements`

| Column                             | Member of `trackers[]` | Channel `inv.<serial>.…` |
| ---------------------------------- | ---------------------- | ------------------------ |
| `mppt_measurements.measurement_id` | not sent               | Parent measurement       |
| `mppt_measurements.tracker_number` | `number`               | `N` in `mppt.N`          |
| `mppt_measurements.dc_power_w`     | `powerW`               | `mppt.N.power` (W)       |
| `mppt_measurements.dc_current_ma`  | `currentMa`            | `mppt.N.current` (mA)    |
| `mppt_measurements.dc_voltage_mv`  | `voltageMv`            | `mppt.N.voltage` (mV)    |

Tracker numbers are not renumbered. A tracker bound to a string is aggregated
for that string.

### `battery_measurements`

| Column                                          | Member of `battery`       | Channel `inv.<serial>.…`    |
| ----------------------------------------------- | ------------------------- | --------------------------- |
| `battery_measurements.measurement_id`           | not sent                  | Parent measurement          |
| `battery_measurements.state_of_charge_permille` | `stateOfChargePermille`   | `battery.soc` (‰)           |
| `battery_measurements.voltage_mv`               | `voltageMv`               | `battery.voltage` (mV)      |
| `battery_measurements.current_ma`               | `currentMa`               | `battery.current` (mA)      |
| `battery_measurements.temperature_millicelsius` | `temperatureMillicelsius` | `battery.temperature` (m°C) |

The system's battery state is the mean state of charge of its batteries.

### `site_consumption_measurements`

| Column                                             | Member of `measurements[].system` | Kept in PVLog as                                 |
| -------------------------------------------------- | --------------------------------- | ------------------------------------------------ |
| `site_consumption_measurements.measured_at`        | `measurements[].measuredAt`       | Time of the reading                              |
| `site_consumption_measurements.consumed_power_w`   | `consumptionPowerW`               | Consumption power of the system                  |
| `site_consumption_measurements.consumed_energy_wh` | `consumptionLifetimeWh`           | Lifetime consumption; interval energy is derived |

`consumed_energy_wh` is the meter's running total, so it is sent as the
lifetime counter and PVLog derives the energy of each interval from it.

### `inverter_energy_samples`

| Column                                    | Member of `energySamples[]` | Kept in PVLog as               |
| ----------------------------------------- | --------------------------- | ------------------------------ |
| `inverter_energy_samples.inverter_id`     | `deviceId` (serial number)  | Device of the sample           |
| `inverter_energy_samples.measured_at`     | `measuredAt`                | Time of the sample             |
| `inverter_energy_samples.total_energy_wh` | `energyLifetimeWh`          | Lifetime counter of the sample |
| `inverter_energy_samples.power_w`         | `powerW`                    | Power of the sample            |

### `inverter_daily_yields`

| Column                                  | Member of `dailySummaries[]` | Kept in PVLog as                                    |
| --------------------------------------- | ---------------------------- | --------------------------------------------------- |
| `inverter_daily_yields.inverter_id`     | `deviceId` (serial number)   | Device of the summary                               |
| `inverter_daily_yields.yield_date`      | `date`                       | Local date                                          |
| `inverter_daily_yields.daily_energy_wh` | `energyWh`                   | Generation of the device on that day                |
| `inverter_daily_yields.total_energy_wh` | `energyLifetimeWh`           | Lifetime counter at the end of the day              |
| `inverter_daily_yields.is_complete`     | `complete`                   | Whether the day will not change any more            |
| `inverter_daily_yields.updated_at`      | not sent                     | Selects the rows to send; PVLog records the receipt |

A day's generation is the sum of the yields of its inverters. A complete sum
is reported in preference to the value computed from readings, which stays
retrievable.

### `inverter_events`

| Column                            | Member of `events[]`       | Kept in PVLog as                 |
| --------------------------------- | -------------------------- | -------------------------------- |
| `inverter_events.inverter_id`     | `deviceId` (serial number) | Device of the event              |
| `inverter_events.device_event_id` | `eventId`                  | Identity of the event            |
| `inverter_events.occurred_at`     | `occurredAt`               | Time of the event                |
| `inverter_events.event_code`      | `code`                     | Event code                       |
| `inverter_events.event_type`      | `type`                     | Event type                       |
| `inverter_events.category`        | `category`                 | Severity category                |
| `inverter_events.event_group`     | `group`                    | Event group                      |
| `inverter_events.tag`             | `message`                  | Event message                    |
| `inverter_events.old_value`       | `oldValue`                 | Value before a change            |
| `inverter_events.new_value`       | `newValue`                 | Value after a change             |
| `inverter_events.user_group`      | `userGroup`                | User group that caused the event |

## What is not exported

smalog's diagnostics (`poll_transmissions`, `inverter_query_support`), its
application log and its migration bookkeeping describe the logger, not the
plant, and are not delivered.

## Testing an export

PVLog and smalog test against one file,
[`smalog-schema-v1.request.json`](../../tests/fixtures/ingest/smalog-schema-v1.request.json):
two inverters, three phases, trackers 1 and 3, a battery, archive samples,
daily yields and events. PVLog's contract test submits it on SQLite and
PostgreSQL and reads every value back. An export that produces this request
from the corresponding rows delivers data PVLog stores without loss.
