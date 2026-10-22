# One image for the master: the Rust binary with the web app built into it. See docker-compose.yml.

FROM node:22-bookworm-slim AS web
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend ./
RUN npm run build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY proto proto
COPY --from=web /src/frontend/dist frontend/dist
RUN cargo build --release -p fafnirx-master -p fafnirx-sign

FROM debian:bookworm-slim
# rclone performs the offsite mirror uploads; ca-certificates lets it (and the master) verify remote TLS; ffmpeg
# (with ffprobe) reads videos and makes their poster pictures and playable copies; curl is for the health check.
RUN apt-get update \
    && apt-get install -y --no-install-recommends rclone ca-certificates ffmpeg curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1000 --create-home fafnirx \
    && mkdir /data /config && chown fafnirx /data /config
COPY --from=build /src/target/release/fafnirx-master /usr/local/bin/fafnirx-master
COPY --from=build /src/target/release/fafnirx-sign /usr/local/bin/fafnirx-sign
# /config holds fafnirx.json (devices, mirrors, sources, settings) and, if you use one, rclone.conf. Map a host folder
# onto it to keep the setup outside the container and to edit it by hand.
ENV FAFNIRX_DATA=/data FAFNIRX_CONFIG=/config/fafnirx.json FAFNIRX_HTTP=0.0.0.0:8080
VOLUME /data /config
EXPOSE 7443 8080
USER fafnirx
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${FAFNIRX_HTTP##*:}/healthz" || exit 1
CMD ["fafnirx-master"]
