# syntax=docker/dockerfile:1.7

FROM rust:1.95.0-bookworm AS build
WORKDIR /build

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY tests ./tests

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
