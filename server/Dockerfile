# syntax=docker/dockerfile:1
#
# PencariMovie Server - Rust/grammers port.
#
#   docker build -f server/Dockerfile -t pencarimovie-rust .
#   docker run -d --name pm-rust -p 8092:8092 -v pm-rust-storage:/app/storage pencarimovie-rust
#
# Build context must be the REPOSITORY ROOT: the dashboard (`public/`) is
# compiled into the binary with `include_dir!` (server/src/assets.rs reads
# `$CARGO_MANIFEST_DIR/../../public`), so `public/` has to be part of the build.
#
# `storage/` is a VOLUME: it holds the bot sessions, auth.json and caches and
# must survive image upgrades and container recreates.

# ── build ───────────────────────────────────────────────────────────────────
FROM rust:bookworm AS build
WORKDIR /src

# Cache the dependency build: only Cargo.{toml,lock} change rarely.
COPY server/Cargo.toml server/Cargo.lock /src/server/
COPY public /src/public
RUN mkdir -p /src/server/src && echo 'fn main() {}' > /src/server/src/main.rs \
    && cargo build --release --manifest-path /src/server/Cargo.toml \
    && rm -rf /src/server/src

# The real sources + the embedded dashboard.
COPY server/src /src/server/src
RUN touch /src/server/src/main.rs \
    && cargo build --release --manifest-path /src/server/Cargo.toml \
    && strip /src/server/target/release/pencarimovie-rust

# ── runtime ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /src/server/target/release/pencarimovie-rust /app/pencarimovie-rust
COPY server/docker/start.sh /app/start.sh
RUN chmod +x /app/pencarimovie-rust /app/start.sh && mkdir -p /app/storage

# PaaS contract: listen on the port the platform gives us, on every interface.
# `PM_PAAS=1` also tells the binary to default to 0.0.0.0 when PORT is unset.
ENV PORT=8092 \
    PM_LISTEN_HOST=0.0.0.0 \
    PM_PAAS=1 \
    PM_APP_ROOT=/app

EXPOSE 8092
VOLUME ["/app/storage"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT}/api/version" > /dev/null || exit 1

ENTRYPOINT ["/app/start.sh"]
