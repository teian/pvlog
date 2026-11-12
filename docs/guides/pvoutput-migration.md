# Moving history from PVOutput

PVLog takes over the history a system collected on PVOutput from the files
PVOutput exports. A file is read in a dry run first; nothing is stored until
the import is committed. Importing the same file again stores nothing twice,
and times for which a logger already delivered data to PVLog keep the
logger's values.

## What can be imported

PVLog reads the two kinds of history PVOutput keeps:

- **Status files**: the readings through the day, as the CSV export of the
  status history or the response of the `getstatus` service. Each row becomes
  a reading of the import source.
- **Output files**: one row per day, as the CSV export of the daily output or
  the response of the `getoutput` service. Each row becomes a daily summary of
  the import source; a day before the current one is marked complete.

A file may have a header row, whose column names may carry their unit, such as
`Energy Generated (kWh)`. Without a header, the columns are read in the field
order of PVOutput's services. Local dates and times are read in the system's
timezone. Which PVLog field each column fills is listed in the
[PVOutput field mapping](../reference/pvoutput-field-mapping.md).

A file may have at most 4 MiB (4194304 bytes). A longer history is exported
and imported in several files, for example one per year.

## Importing in the web interface

1. Open the system, choose **Settings** and then **Data sources**.
2. If the system has no data source with the **Import** profile yet, add one:
   pick **Import** and choose **Create data source**. It needs no key.
3. Under the import source, choose the file in **Export file (CSV)** and then
   **Start dry run**.
4. The dry run shows how many rows the file has, how many can be imported, the
   period they cover and how many overlap data that is already stored. Every
   rejected row is listed with its row number, column and reason.
5. Commit the import to store the rows. The result shows how many rows were
   stored, how many were already present and how many were skipped because
   another data source holds that time.

A dry run that is not committed is removed after 24 hours.

## Rejected rows

A row is rejected, and the rest of the file is still imported, when

- its date or time cannot be read, or the time does not exist in the
  system's timezone because the clocks go forward;
- the same time (status files) or day (output files) occurs a second time;
- a value is negative, not a number, outside the range PVLog stores, or has a
  unit PVLog does not know;
- a weather condition is longer than 40 characters.

When the clocks go back, a time that occurs twice is read as its first
occurrence and, the second time it appears, as its second occurrence.

## Overlap with other data

Imported readings and summaries belong to the import source. A time for which
another data source already delivered a reading, or a day it already
summarized, is skipped and that data is kept. Rows the import source holds
already in the same form count as already present. Committing an import a
second time returns the first result unchanged.

## Importing through the API

The import is available to scripts with an API key that has the `systems:write`
scope and is not restricted to one system:

1. `POST /api/v1/systems/{system_id}/data-sources/{data_source_id}/imports`
   with the JSON body `{"fileName": "...", "content": "..."}` reads the file in
   a dry run and answers `201` with its report. The body may be compressed
   with gzip, named in `Content-Encoding`. A file over the limit is refused
   with `413`, and the problem states the limit in `limit`.
2. `POST /api/v1/systems/{system_id}/data-sources/{data_source_id}/imports/{import_id}/commit`
   stores what the dry run accepted.
3. `GET /api/v1/systems/{system_id}/data-sources/{data_source_id}/imports`
   lists the dry runs and imports of the source, newest first.

The request and response members are described in the
[API reference](../../openapi/pvlog-v1.yaml).
