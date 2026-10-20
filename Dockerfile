# syntax=docker/dockerfile:1
#
# PencariMovie Server - Alpine (musl) image.
#
#   docker build -f Dockerfile.rust -t pencarimovie-server .
#   docker run --rm -p 8088:8088 -v pmdata:/data pencarimovie-server
#
# The whole server is ONE binary: `public/` is embedded at compile time
# (`include_dir!`), so the runtime image needs nothing but the executable and a
# CA bundle. `heroku.yml` builds this same file for Heroku; the bind address is
# automatic there (the binary binds 0.0.0.0 whenever it sees a PaaS marker:
# DYNO, RENDER, K_SERVICE, or PORT without an explicit host).
#
# State (Telegram sessions, auth.json, caches, logs) lives in PM_APP_ROOT and is
# the only thing worth a volume.

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
# The crate embeds ../../public, so the build context must keep the repo layout.
COPY port/rust ./port/rust
COPY public ./public
WORKDIR /src/port/rust
RUN cargo build --release --locked

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata curl \
 && adduser -D -u 10001 -h /data pencarimovie \
 && mkdir -p /data \
 && chown -R 10001:10001 /data

COPY --from=build /src/port/rust/target/release/pencarimovie-server /usr/local/bin/pencarimovie-server

ENV PM_APP_ROOT=/data \
    PM_LISTEN_HOST=0.0.0.0 \
    PORT=8088

EXPOSE 8088
VOLUME ["/data"]
USER 10001

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s \
  CMD curl -fsS "http://127.0.0.1:${PORT}/api/version" >/dev/null || exit 1

CMD ["pencarimovie-server"]
