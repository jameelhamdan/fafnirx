# One image for the master (the Rust binary with the web app built into it) and for the optional streaming service
# (fafnirx-stream, which needs the same ffmpeg). See docker-compose.yml.

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
RUN cargo build --release -p fafnirx-master -p fafnirx-sign -p fafnirx-stream

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
# The streaming service runs from this same image (see the `stream` service in docker-compose.yml).
COPY --from=build /src/target/release/fafnirx-stream /usr/local/bin/fafnirx-stream
# /config holds rclone.conf, if you use one for mirrors. Map a host folder onto it.
ENV FAFNIRX_DATA=/data FAFNIRX_RCLONE_CONFIG=/config/rclone.conf FAFNIRX_HTTP=0.0.0.0:8080
VOLUME /data /config
EXPOSE 7443 8080
USER fafnirx
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${FAFNIRX_HTTP##*:}/healthz" || exit 1
CMD ["fafnirx-master"]
