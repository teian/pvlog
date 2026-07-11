# PVLog Compose profiles

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
