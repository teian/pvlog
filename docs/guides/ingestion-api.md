# Ingestion API

The ingestion API is the way a logger delivers data to PVLog. One request
carries what the logger has: its devices, their measurements, archive energy
samples, daily summaries and events. The same request serves the regular
cycle, catching up after an outage and the first delivery of years of history.

The [observation resources](uploader-integration.md) remain as the minimal
form for a script that sends one reading at a time. Both feed the same storage.

| Operation                               | Purpose                                 |
| --------------------------------------- | --------------------------------------- |
| `POST /api/v1/systems/{id}/ingest`      | Deliver data                            |
| `GET /api/v1/systems/{id}/ingest/state` | Read how far the data has arrived       |
| `GET /api/v1/ingest/capabilities`       | Read schema versions, limits, encodings |

All three accept the key of a data source as `Authorization: Bearer <key>`.
That key carries `telemetry:write` for one system and can read nothing else.

## The request

```json
{
  "schemaVersion": 1,
  "source": { "name": "smalog", "version": "0.3.0" },
  "devices": [
    { "id": "2000123456", "model": "STP 6.0-3SE-40", "firmware": "3.10.18.R" }
  ],
  "measurements": [
    {
      "measuredAt": 1800000000,
      "system": { "consumptionPowerW": 420 },
      "devices": [
        {
          "id": "2000123456",
          "ac": [{ "phase": 1, "powerW": 800, "voltageMv": 230100 }],
          "energyLifetimeWh": 10000000,
          "trackers": [{ "number": 1, "powerW": 1300 }],
          "battery": { "stateOfChargePermille": 815 }
        }
      ]
    }
  ],
  "energySamples": [
    {
      "deviceId": "2000123456",
      "measuredAt": 1799999700,
      "energyLifetimeWh": 9999800
    }
  ],
  "dailySummaries": [
    {
      "deviceId": "2000123456",
      "date": "2027-01-14",
      "energyWh": 5200,
      "complete": true
    }
  ],
  "events": [
    {
      "deviceId": "2000123456",
      "eventId": 8812,
      "occurredAt": 1799996400,
      "code": 10100
    }
  ]
}
```

Every list is optional. The complete schema is `IngestRequest` in the
[API reference](../../openapi/pvlog-v1.yaml); a complete request is the
[contract fixture](../../tests/fixtures/ingest/smalog-schema-v1.request.json).

Three rules hold for every value:

- **Units are in the name.** Values are integers in the unit their member name
  ends with: `W`, `Wh`, `Mv` (mV), `Ma` (mA), `Mhz` (mHz), `S` (seconds),
  `Permille` (‰) and `Millicelsius` (m°C). Nothing is converted or rounded.
- **Times are epoch seconds.** A daily summary names its local date instead.
- **Absent means missing.** A member that is left out is stored as missing and
  never read as zero. Send no member for a value the device did not report.

Tracker numbers are the numbers the device uses; gaps are kept. Status values
are the numeric codes of the device and are labelled when they are shown.

A value PVLog has no member for goes into `extra` of its device, with a unit:

```json
"extra": { "dc.isolation": { "value": 1.25, "unit": "MΩ" } }
```

It becomes the measurement channel `inv.<serial>.x.dc.isolation`.

## What PVLog derives

Device values are stored as measurement channels under the keys of the
[source profiles](uploader-integration.md). The logger does not need to know
those keys or to compute totals. For each measurement PVLog derives:

| System value           | Derived from                                             |
| ---------------------- | -------------------------------------------------------- |
| Generation power       | Sum of the AC power of every phase that reported one     |
| Lifetime generation    | Sum of the lifetime counters of the devices              |
| Interval generation    | What those counters grew by since the measurement before |
| Grid import and export | Sum over the devices that report them                    |
| Battery state          | Mean state of charge of the batteries that report one    |

A value stated in `system` wins over the derived one. A value no device
reported stays missing. Interval energy is not derived across a counter that
went back, nor across a change in the set of devices that report: when one of
two inverters is silent, the system's power is the other inverter's power, and
the energy of that interval is unknown rather than too small.

A source without device detail sends `system` alone. Its lifetime counters are
turned into interval energy in the same way.

Archive energy samples are stored for their device. For a time that has no
measurement, the samples of that time supply the system's power and interval
energy. A measurement that arrives later for the same time replaces what the
samples supplied; samples never replace a measurement.

## Sending again is safe

Items are identified by what they describe, not by a key the client invents:

| Item          | Identity                                                 |
| ------------- | -------------------------------------------------------- |
| Measurement   | Data source and `measuredAt`                             |
| Energy sample | Device and `measuredAt`                                  |
| Daily summary | Data source, device and `date`                           |
| Event         | Device and `eventId`, or its time and code without an id |

An item that is sent again as it is stored changes nothing and is counted as
`unchanged`. An item that is sent with other values replaces what is stored and
is counted as `stored`. That is how the yield of a running day matures until it
is marked `complete`. No `Idempotency-Key` header is used.

## The answer is a cursor

```json
{
  "measurements": { "stored": 1, "unchanged": 0, "rejected": [] },
  "energySamples": { "stored": 1, "unchanged": 0, "rejected": [] },
  "dailySummaries": { "stored": 1, "unchanged": 0, "rejected": [] },
  "events": { "stored": 1, "unchanged": 0, "rejected": [] },
  "marks": {
    "system": { "measuredAt": 1800000000 },
    "devices": [
      {
        "deviceId": "2000123456",
        "measuredAt": 1800000000,
        "sampleAt": 1799999700,
        "summaryDate": "2027-01-14",
        "eventAt": 1799996400
      }
    ]
  }
}
```

The marks say, per device and kind of data, up to where PVLog holds what this
data source delivered. A logger advances its own position to them. A mark never
moves back. A logger that lost its position asks `ingest/state` and receives
the same marks.

## When a request is refused

A request is checked completely before anything is stored. When it is refused,
nothing of it was stored. The answer is a problem document
(`application/problem+json`):

| Status | `title`                        | Meaning                                                              |
| ------ | ------------------------------ | -------------------------------------------------------------------- |
| 400    | `malformed_request`            | The body is not JSON of the expected form, or cannot be decompressed |
| 413    | `limit_exceeded`               | A list or the body is larger than the limit named in `limit`         |
| 415    | `unsupported_content_encoding` | The encoding is not one of `contentEncodings`                        |
| 422    | `unknown_member`               | `detail` is the path of a member the schema version does not define  |
| 422    | `unsupported_schema_version`   | `schemaVersions` lists the versions the server accepts               |
| 422    | `items_rejected`               | `rejected` lists every item that cannot be stored                    |

Each entry of `rejected` names the list, the position of the item in it, the
member and a reason:

```json
{
  "category": "measurements",
  "index": 0,
  "member": "devices[0].battery.stateOfChargePermille",
  "reason": "out_of_range"
}
```

A logger drops exactly those items and sends the rest again. Unknown members
are refused instead of ignored, so that a logger learns on its first request
when PVLog would not keep a value.

A `200` answer can carry entries in `rejected` too. They name single values
that were left out while the rest of their measurement was stored, for example
an `extra` value whose unit contradicts the unit its channel already has.

Authentication, rate limits and overload behave as for every other request:
`401`, `403` and `404` mean that the key or the system is not accepted, and
`429` and `503` ask to retry after the time in `Retry-After`.

## Limits and compression

`GET /api/v1/ingest/capabilities` returns what the server accepts:

```json
{
  "schemaVersions": [1],
  "contentEncodings": ["identity", "gzip", "zstd"],
  "limits": {
    "measurements": 5000,
    "energySamples": 20000,
    "dailySummaries": 5000,
    "events": 5000,
    "devices": 64,
    "extraValuesPerDevice": 32,
    "bodyBytes": 1048576,
    "decodedBodyBytes": 33554432
  }
}
```

A body may be compressed with `Content-Encoding: gzip` or `zstd`. `bodyBytes`
limits the body as it is sent and `decodedBodyBytes` the body after
decompression, so a request that carries thousands of measurements has to be
compressed. Read the limits instead of assuming them; they may differ between
servers.
