# Test fixtures

Versioned deterministic fixtures shared by contract, integration, and end-to-end
tests live below this directory. Secrets and production-derived personal data are prohibited.

## `ingest/`

- `smalog-schema-v1.request.json` is a request to the ingestion API as smalog's
  PVLog export builds it from its schema version 1: two inverters, three
  phases, tracker numbers with a gap, a battery, archive energy samples, daily
  yields and events. PVLog's contract test submits it and reads every value
  back; the smalog repository tests its export against the same file.
- `smalog-schema-v1.columns.json` lists the plant tables and columns of that
  schema. The documentation check requires each of them in the column mapping.
