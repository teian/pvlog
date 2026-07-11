# PVLog

PVLog is a self-hosted platform for collecting, analyzing, and operating
photovoltaic systems through a modern API and web application. It supports a
SQLite management/per-account topology for smaller installations and PostgreSQL
for larger deployments.

## Local development

### Prerequisites

- Rust 1.95.0; `rust-toolchain.toml` installs the required `rustfmt` and Clippy
  components automatically through rustup
- Node.js 24 or newer and Corepack
- pnpm 11.1.1, selected from `package.json`
- SQLite development/runtime libraries; PostgreSQL 17 when developing against
  the PostgreSQL profile
- Chromium installed by Playwright when running browser tests

Install the JavaScript dependencies from the repository root:

```sh
corepack enable
pnpm install --frozen-lockfile
```

### Start the backend with SQLite

Development defaults use `data/management.sqlite3`, `data/accounts/`, local
authentication, `127.0.0.1:18087`, and non-secure development cookies. Copy the
example configuration and replace both cryptographic secrets with independent
values containing at least 32 bytes:

```sh
cp .env.example .env
# Edit .env and generate each secret with: openssl rand -hex 32
mkdir -p data/accounts

cargo run -p pvlog -- migrate apply
cargo run -p pvlog -- server
```

The backend reads an optional `.env` in its working directory as well as an
optional `pvlog.toml`. Configuration priority is: built-in defaults,
`pvlog.toml`, `.env`, then variables already present in the process environment.
Use nested `PVLOG_*` keys separated by double underscores, as shown in
`.env.example`. Keep `.env` and `pvlog.toml` private; both are ignored by Git.

Run the worker in a second terminal with the same environment variables:

```sh
cargo run -p pvlog -- worker --interval-seconds 5
```

For a single worker cycle, use `pnpm backend:worker`. The repository aliases
`pnpm backend:server` and `cargo run-server` also start the API server.

To use PostgreSQL locally, set the backend and connection URL before applying
migrations and starting the processes:

```sh
export PVLOG_DATABASE__BACKEND=postgres
export PVLOG_DATABASE__POSTGRES__URL=postgres://pvlog:pvlog@127.0.0.1:5432/pvlog
cargo run -p pvlog -- migrate apply
cargo run -p pvlog -- server
```

### Start the web application

The Vite development server runs on `http://localhost:5173`:

```sh
pnpm dev
```

The browser runtime uses the same-origin `/api/v1` base URL. During development,
Vite automatically proxies that path to `http://127.0.0.1:18087`, so start the
Rust backend before opening the UI. Override only the proxy target when the API
runs elsewhere:

```sh
VITE_DEV_API_TARGET=http://127.0.0.1:19000 pnpm dev
```

Optional browser tracing is disabled by default. Enable it only with a trusted
OTLP/HTTP collector:

```sh
VITE_OTEL_ENABLED=true \
VITE_OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=http://localhost:4318/v1/traces \
pnpm dev
```

### Quality checks

Common checks are:

```sh
cargo fmt --all --check
cargo check-all
cargo clippy-all
cargo test-all

pnpm lint
pnpm typecheck
pnpm test:ui:coverage
pnpm test:e2e
pnpm openapi:lint
pnpm openapi:routes
pnpm openapi:compare
pnpm test:docs
pnpm build
```

Install the Playwright browser once before the end-to-end suite:

```sh
pnpm exec playwright install chromium
```

## Production setup

The recommended deployment uses the unprivileged container image and one of the
Compose profiles in [`deploy/compose`](deploy/compose/README.md). The profiles
run migrations before starting the server and worker, persist database data,
and expose health checks through `pvlog doctor --json`.

### 1. Build or select an immutable image

Build the image locally, or replace the tag with an immutable registry release:

```sh
docker build --tag pvlog:0.1.0 .
```

Do not use `latest` for an upgradeable installation.

### 2. Configure secrets and the public URL

```sh
cd deploy/compose
cp .env.example .env
```

Edit `.env` and set at least:

- `PVLOG_IMAGE` to the immutable image tag
- `PVLOG_SESSION_SECRET` and `PVLOG_CREDENTIAL_ENCRYPTION_KEY` to independently
  generated values from `openssl rand -hex 32`
- `PVLOG_PUBLIC_BASE_URL` to the externally visible HTTPS URL
- the PostgreSQL database/user/password values when using PostgreSQL

Keep `.env` private. Configure OIDC/OAuth2 connectors through provider-neutral
`PVLOG_AUTH__CONNECTORS__<INDEX>__*` variables or a mounted `pvlog.toml`, and
keep connector client secrets in the deployment secret store.

### 3. Start one database profile

For SQLite, which stores the management database and opaque account databases in
a persistent volume:

```sh
docker compose --env-file .env --profile sqlite up -d
docker compose --env-file .env --profile sqlite ps
docker compose --env-file .env --profile sqlite logs sqlite-migrate
```

For PostgreSQL 17:

```sh
docker compose --env-file .env --profile postgres up -d
docker compose --env-file .env --profile postgres ps
docker compose --env-file .env --profile postgres logs postgres-migrate
```

Use exactly one profile for an installation. The API is published on
`PVLOG_HTTP_PORT`, which defaults to `8080`.

### 4. Put an HTTPS reverse proxy in front

Terminate TLS at a trusted reverse proxy and forward requests to the PVLog API.
Preserve `Host`, forwarding headers, and `x-request-id`; restrict direct access
to the backend port. Production mode requires an HTTPS public base URL and
secure session cookies.

The production image builds the React application first and embeds `dist/ui/`
directly into the Rust executable with `rust-embed`. The PVLog server therefore
serves the SPA, hashed assets, runtime configuration, bundled API reference, and
`/api/v1/` from one origin. No separate web server or writable UI volume is
required; the reverse proxy only needs to forward the public origin to PVLog.

Direct local `cargo build --release` uses the tracked development placeholder.
To create a release binary outside Docker, run `pnpm build`, replace the contents
of `embedded-ui/` with `dist/ui/`, and then run `cargo build --release -p pvlog`.

### 5. Verify and operate the deployment

```sh
docker compose --env-file .env --profile sqlite exec sqlite-server pvlog doctor --json
```

Replace `sqlite`/`sqlite-server` with `postgres`/`postgres-server` for the
PostgreSQL profile. Before upgrades, create and verify a backup, update only the
immutable image tag, start the profile, and confirm schema compatibility and all
health checks. See the operator guide for export/import, restore drills,
maintenance, observability, and rollback procedures.

## Documentation

- [Documentation index](docs/README.md)
- [Developer quickstarts](docs/guides/developer-quickstarts.md)
- [Local authentication and RBAC quickstart](docs/guides/local-authentication-rbac.md)
- [Operator and recovery guide](docs/guides/operator-recovery.md)
- [Uploader integration and functional coverage](docs/guides/uploader-integration.md)
- [OpenAPI contract](openapi/pvlog-v1.yaml)
