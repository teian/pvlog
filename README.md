# pvlog

PVLog is a self-hosted platform for collecting, analyzing, and operating photovoltaic systems through a modern API and web application.

## Guides

- [Local authentication and RBAC quickstart](docs/guides/local-authentication-rbac.md)
- [Documentation index](docs/README.md)
- [Developer quickstarts](docs/guides/developer-quickstarts.md)
- [Operator and recovery guide](docs/guides/operator-recovery.md)
- [Uploader integration and functional coverage](docs/guides/uploader-integration.md)

## Operator checks

Run `pvlog doctor` before a rollout or after restoring a database. It performs a
read-only reachability and schema-compatibility check; use `pvlog doctor --json`
for machine-readable automation output.

## Container image

Build the unprivileged runtime image locally:

```sh
docker build --tag pvlog:local .
```

The image runs `pvlog server` by default and exposes the same command surface
for operators, for example `docker run --rm pvlog:local doctor --json`.

## Compose deployment

The [Compose profiles](deploy/compose/README.md) provide separate SQLite and
PostgreSQL deployments with explicit migrations, health checks, persistent data,
and provider-neutral authentication configuration. Copy
`deploy/compose/.env.example` to a private `deploy/compose/.env` before use.
