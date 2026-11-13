# Demo data

For trying PVLog locally, a demo data set is built once from a real SBFspot
database and loaded into `data/` whenever a clean demo is wanted. A simulator
keeps one of its systems live.

## What the demo contains

- **Demo SBFspot**: the history of the SBFspot database, delivered by
  `pvlog-sbfspot-push` as a real installation would. The history is moved by
  whole years so that it reaches up to the day the demo was built, with the
  seasons in place. SBFspot archives only while the inverters feed in; the
  demo adds readings without generation for the night of each day that has
  data, so that a simulated household's consumption is known around the clock.
  Days without any data stay gaps. The inverters are adopted from the
  discovery proposals with one string per tracker; a string's capacity is
  estimated from the largest DC power its tracker delivered. An import data
  source is ready for trying the history import.
- **Demo Live**: a simulated hybrid inverter with two strings, a 10 kWh
  battery and a grid meter, delivered through the
  [ingestion API](ingestion-api.md) as smalog would. Its generation follows
  the real generation of the SBFspot system on the same calendar day, scaled
  to its size; a second household, the battery and the grid follow from that.
  It has a year of history at five-minute intervals and the last day at
  one-minute intervals.
- **PVOutput files** in `demo-data/pvoutput/`: a year of daily output before
  the SBFspot history starts, single status days before it, and one status day
  that overlaps it. Import them into the import data source as described in
  [Moving history from PVOutput](pvoutput-migration.md); the overlapping day
  shows how data of another source is kept.

Everything is derived deterministically, so building twice from the same
database on the same day gives the same values. The demo user is
`demo@pvlog.local` with the password `demo-password`; use it locally only.

## Building

```sh
bun run demo:build --sbfspot /path/to/SBFspot.db
```

The database is copied and never changed. Its write-ahead log is not copied: a
log taken from a synchronised or running installation can be older than the
database and would damage the copy. The build fails if the copy is damaged.

The build starts its own PVLog instance on port 18197, so the development
server can keep running. It writes the snapshot to `demo-data/snapshot/`,
which Git ignores because it holds the real history of the SBFspot
installation. Options: `--out` for another directory, `--live-days` for the
history of the simulated system, `--latitude` and `--longitude` for the
location of both systems, `--port` for the build instance. The database can
also be named in `PVLOG_DEMO_SBFSPOT_DB`.

## Loading

Stop the server, then:

```sh
bun run demo:load
```

`data/` is replaced by the snapshot. Data in `data/` that did not come from
the demo is moved aside to `data.bak-<time>` first; an earlier demo is simply
replaced. The snapshot itself stays unchanged, so it can be loaded as often
as needed.

The snapshot uses the secrets of `.env` it was built with: the keys of its
data sources only work with the same `PVLOG_SECURITY__*` values.

## Keeping a system live

With the server running:

```sh
bun run demo:simulate
```

The simulator first delivers everything since the snapshot was built, then
one measurement of **Demo Live** per minute. It continues where it stopped
when it is started again; loading the demo resets it. `--interval` changes
the cadence in seconds and `--once` only catches up. **Demo SBFspot** ends
where the snapshot was built; build the demo again to move it up to the
present.
