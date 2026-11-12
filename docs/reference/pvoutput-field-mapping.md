# PVOutput field mapping

PVLog covers the data that PVOutput holds for a system. This reference lists
every field of PVOutput's status, output and system services with the PVLog
field that carries the same information. PVLog does not speak PVOutput's
upload protocol: a logger sends its data through the PVLog API (see
[Uploader integration](../guides/uploader-integration.md)), and existing
history is brought over by an import.

Units differ in places. PVLog uses integer SI sub-units throughout: watts,
watt-hours, millivolts and thousandths of a degree Celsius. Timestamps are
epoch milliseconds in UTC; local dates are given in the system's timezone.

## Status (`addstatus`, `getstatus`)

A PVOutput status is a PVLog reading, sent to
`POST /api/v1/systems/{system_id}/observations`.

| PVOutput     | Meaning            | PVLog field                                                                                | Notes                                                                                                                                                |
| ------------ | ------------------ | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `d`, `t`     | Date and time      | `observedAtEpochMillis`                                                                    | One UTC instant instead of a local date and time                                                                                                     |
| `v1`         | Energy generation  | `generationEnergyWh`, or `generationLifetimeWh` for a counter                              | See `c1` for cumulative values                                                                                                                       |
| `v2`         | Power generation   | `generationPowerWatts`                                                                     |                                                                                                                                                      |
| `v3`         | Energy consumption | `consumptionEnergyWh`, or `consumptionLifetimeWh` for a counter                            |                                                                                                                                                      |
| `v4`         | Power consumption  | `consumptionPowerWatts`                                                                    |                                                                                                                                                      |
| `v5`         | Temperature        | `temperatureMillidegreesCelsius`                                                           | 23.4 °C is `23400`; rollups carry the minimum and maximum                                                                                            |
| `v6`         | Voltage            | `voltageMillivolts`                                                                        | 239.5 V is `239500`; rollups carry the mean                                                                                                          |
| `c1`         | Cumulative flag    | `generationLifetimeWh`, `consumptionLifetimeWh`                                            | Separate fields instead of a flag: interval energy is derived from the difference to the previous counter, and a lower counter is treated as a reset |
| `n`          | Net flag           | `gridImportPowerWatts`, `gridExportPowerWatts`, `gridImportEnergyWh`, `gridExportEnergyWh` | Import and export are separate, non-negative values instead of one signed net value                                                                  |
| `v7` – `v12` | Extended values    | `channels` with `value` and `unit`                                                         | Each extended value is a measurement channel under a key of the source's choosing; decimal values are stored without rounding                        |
| `m1`         | Text message       | `message`                                                                                  | At most 30 characters                                                                                                                                |

## Output (`addoutput`, `getoutput`)

A PVOutput daily output is a PVLog daily summary, sent to
`PUT /api/v1/systems/{system_id}/daily-summaries/{date}`.

| PVOutput | Meaning              | PVLog field                         | Notes                                           |
| -------- | -------------------- | ----------------------------------- | ----------------------------------------------- |
| `d`      | Output date          | `{date}` in the path                | Local date in the system's timezone             |
| `g`      | Energy generated     | `generationEnergyWh`                |                                                 |
| `e`      | Energy exported      | `exportEnergyWh`                    |                                                 |
| `pp`     | Peak power           | `peakPowerWatts`                    |                                                 |
| `pt`     | Peak time            | `peakTime`                          | Local time as `HH:MM`                           |
| `cd`     | Condition            | `weatherCondition`                  | Kept as text, at most 40 characters             |
| `tm`     | Minimum temperature  | `temperatureMinMillidegreesCelsius` |                                                 |
| `tx`     | Maximum temperature  | `temperatureMaxMillidegreesCelsius` |                                                 |
| `cm`     | Comments             | not carried over                    | Free-text comments on a day have no counterpart |
| `ip`     | Import peak          | `importEnergyWhByBand.peak`         |                                                 |
| `io`     | Import off-peak      | `importEnergyWhByBand.offPeak`      |                                                 |
| `is`     | Import shoulder      | `importEnergyWhByBand.shoulder`     |                                                 |
| `ih`     | Import high shoulder | `importEnergyWhByBand.highShoulder` |                                                 |
| `c`      | Consumption          | `consumptionEnergyWh`               |                                                 |

A summary is marked with `complete` once the day will not change any more. For
each day PVLog reports the complete supplied summary first, otherwise the value
computed from readings that cover at least 80 % of the day, otherwise the
incomplete supplied summary; `GET /api/v1/systems/{system_id}/daily-summaries`
returns the reported value together with its basis and both origins. Days that
exist only as a supplied summary are part of the monthly, yearly and seasonal
statistics, which state how many of their days rest on supplied summaries.

## System (`getsystem`)

PVOutput's system record corresponds to the PVLog system with its inverters,
strings and tariffs.

| PVOutput                    | PVLog field                                            | Notes                                                                                             |
| --------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| System name                 | System `name`                                          |                                                                                                   |
| System size                 | System `declaredCapacityWatts`                         | Once strings are entered, the capacity is the sum of their panels and replaces the declared value |
| Postcode                    | System `locationLabel`                                 | A free-text place; the position is in the coordinates                                             |
| Number of panels            | String `panelCount`                                    |                                                                                                   |
| Panel power                 | String `modulePeakPowerWatts`                          |                                                                                                   |
| Panel brand                 | String `panelManufacturer`, `panelModel`               |                                                                                                   |
| Number of inverters         | One inverter record per device                         |                                                                                                   |
| Inverter power              | Inverter `ratedPowerWatts`                             |                                                                                                   |
| Inverter brand              | Inverter `manufacturer`, `model`                       |                                                                                                   |
| Orientation                 | String `orientationDegrees`                            | Degrees from north instead of a compass point                                                     |
| Array tilt                  | String `tiltDegrees`                                   |                                                                                                   |
| Shade                       | String `shading`                                       | Time windows with a loss share instead of one of four levels                                      |
| Install date                | System `commissioningDate`                             |                                                                                                   |
| Latitude, longitude         | System `latitudeMicrodegrees`, `longitudeMicrodegrees` |                                                                                                   |
| Status interval             | System `statusIntervalSeconds`                         | Seconds instead of minutes                                                                        |
| Secondary array             | A second string of the inverter                        | With its own panel count, panel power, orientation and tilt                                       |
| Export tariff               | Tariff with direction `export`                         | Price per kWh in the currency's minor unit, effective-dated                                       |
| Import tariffs by band      | Tariff with direction `import`                         | One import price per period; the imported energy by band is kept in the daily summaries           |
| Daily charge                | not carried over                                       | PVLog has no fixed daily charge                                                                   |
| Teams, donations            | not carried over                                       | Community features of PVOutput                                                                    |
| Extended data configuration | Measurement channel name, unit and assignment          | Edited under the system's Settings → Measurement channels                                         |
