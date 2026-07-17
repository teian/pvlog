# PVLog Compose profiles

## PV yield forecasting

Forecasting is disabled by default. Set `PVLOG_FORECASTING_ENABLED=true` only
after deploying a normalized weather adapter and an external secret reference.
`PVLOG_FORECASTING_ADAPTER_ENDPOINT` identifies the provider-neutral adapter;
`PVLOG_FORECASTING_CREDENTIAL_SECRET_REF` is a reference such as
`secret://weather/production`, never the credential value. The adapter owns
provider authentication and licensing compliance.

Polling defaults to 900 seconds and requests a 72-hour horizon. A stale run may
be used only when its age is within
`PVLOG_FORECASTING_MAXIMUM_STALE_AGE_SECONDS`; set that value to `0` to fail
closed instead. `MODEL_IDENTIFIER` plus `MODEL_REVISION` select deterministic
calculation semantics. Unreferenced working inputs/results expire after
`WORKING_RETENTION_DAYS`, while issued forecasts referenced by historical
performance are preserved. `WORKER_CONCURRENCY` bounds forecast jobs per worker
independently of the worker readiness interval.

Feature gating does not affect telemetry ingestion. If the adapter is down,
forecast endpoints report stale or unavailable modeled data while push and
historical telemetry paths continue operating.

Copy `.env.example` to `.env` and replace every placeholder before starting a
profile. Keep `.env` private: it contains session, credential-encryption, and
database secrets.

Generate the two PVLog secrets independently:

```sh
openssl rand -hex 32
```

Start the SQLite profile, which persists `management.sqlite3` and opaque
per-account database files in the `sqlite-data` volume:

```sh
docker compose --env-file .env --profile sqlite up -d
```

Start the PostgreSQL profile instead:

```sh
docker compose --env-file .env --profile postgres up -d
```

Both profiles run an explicit migration container before the server and worker.
The server and worker health checks execute `pvlog doctor --json`; inspect a
failed migration with:

```sh
docker compose --env-file .env --profile sqlite logs sqlite-migrate
```

Use an immutable `PVLOG_IMAGE` tag for upgrades. Take and verify a backup before
changing that tag, run the profile, and confirm `doctor --json` reports a
compatible schema. Connector settings stay provider-neutral: configure generic
OIDC/OAuth2 records through supported `PVLOG_AUTH__CONNECTORS__<INDEX>__*`
variables or a mounted `pvlog.toml`; store the corresponding secret references
outside version control.

## Reverse-proxy logging and API secrets

Terminate trusted HTTPS before PVLog and preserve the request path. Uploaders
authenticate with `Authorization: Bearer`; the proxy must never log or copy that
header into diagnostic response headers. Do not log request headers, request
bodies, or upstream debug dumps.

PVLog records normalized route templates and safe credential IDs. Verify proxy
behavior before production with a disposable account API key, capture
logs/traces, and confirm that the bearer value does not occur. Revoke the key
after the test. Treat any accidentally logged value as leaked and follow
[the uploader guide](../../docs/guides/uploader-integration.md#rotation-revocation-and-recovery).
