# syntax=docker/dockerfile:1.7

FROM node:24-bookworm-slim AS ui-build
WORKDIR /build

RUN corepack enable

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile

COPY tsconfig.json tsconfig.app.json tsconfig.node.json vite.config.ts ./
COPY openapi ./openapi
COPY src/ui ./src/ui

RUN pnpm build

FROM rust:1.95.0-bookworm AS build
WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY tests ./tests
COPY embedded-ui ./embedded-ui
COPY --from=ui-build /build/dist/ui/ ./embedded-ui/

RUN cargo build --release --locked --package pvlog

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 pvlog \
    && useradd --system --uid 10001 --gid pvlog --create-home --home-dir /var/lib/pvlog pvlog \
    && install --directory --owner pvlog --group pvlog /var/lib/pvlog/data

COPY --from=build /build/target/release/pvlog /usr/local/bin/pvlog

USER pvlog
WORKDIR /var/lib/pvlog

EXPOSE 8080

ENTRYPOINT ["pvlog"]
CMD ["server"]
