# Targets from one build (see docker-compose.yml):
#   master        the Rust binary with the web app built into it; no ffmpeg, so it is small. This is what `docker build .` makes.
#   video         the video module as a container of its own (fafnirx-video), which needs ffmpeg; see docs/video.md.
#   plugin-video  the video module as a plugin bundle (a zip with a checksum) for the master to install; made with
#                 `docker buildx build --target plugin-video --build-arg VERSION=<x.y.z> --output type=local,dest=out .`
# Another module is another pair of targets here, on the same build stages.

FROM node:22-bookworm-slim AS web
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend ./
# The app, and the part of each module that runs in the browser (dist-modules/<id>).
RUN npm run build && npm run build:module

FROM rust:1-bookworm AS build
# The typed queries (crates/db) are made while building, from the SQL and the migrations, and checked against a real
# PostgreSQL. One runs in this stage, for the length of the build only; it is not part of the image.
RUN apt-get update && apt-get install -y --no-install-recommends postgresql && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY modules modules
COPY queries queries
COPY proto proto
COPY --from=web /src/frontend/dist frontend/dist
RUN service postgresql start \
    && until pg_isready -h localhost -q; do sleep 1; done \
    && su postgres -c "psql -qc \"ALTER USER postgres PASSWORD 'build'\"" \
    && FAFNIRX_CODEGEN_DATABASE_URL=postgresql://postgres:build@localhost:5432 \
       cargo build --release -p fafnirx-master -p fafnirx-sign -p fafnirx-video \
    && service postgresql stop

FROM debian:bookworm-slim AS video
# ffmpeg (with ffprobe) cuts the segments, reads the videos and draws their poster frames; curl is for the health check.
RUN apt-get update     && apt-get install -y --no-install-recommends ffmpeg curl     && rm -rf /var/lib/apt/lists/*     && useradd --uid 1000 --create-home fafnirx     && mkdir /data /cache && chown fafnirx /data /cache
COPY --from=build /src/target/release/fafnirx-video /usr/local/bin/fafnirx-video
COPY --from=web /src/frontend/dist-modules/video /usr/share/fafnirx-video/ui
ENV FAFNIRX_DATA=/data FAFNIRX_VIDEO_CACHE=/cache FAFNIRX_MODULE_LISTEN=0.0.0.0:8090 FAFNIRX_MODULE_UI=/usr/share/fafnirx-video/ui
VOLUME /cache
EXPOSE 8090
USER fafnirx
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3     CMD curl -fsS "http://127.0.0.1:${FAFNIRX_MODULE_LISTEN##*:}/health" || exit 1
CMD ["fafnirx-video"]

FROM debian:bookworm-slim AS bundle-video
# The video module as a plugin: its program, its part of the web app and a static ffmpeg, in one zip that the master unpacks
# into <data>/plugins/video. The program is built on the same Debian as the master image, so it runs there.
ARG TARGETARCH
ARG VERSION
ARG FFMPEG=6.0.1
ARG FFMPEG_SHA256_amd64=28268bf402f1083833ea269331587f60a242848880073be8016501d864bd07a5
ARG FFMPEG_SHA256_arm64=7dbd8e2f47bd83de591b9d6ea70e67d32d9aa97e7d47ae402b60c2fe3fd4d0ab
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates xz-utils zip && rm -rf /var/lib/apt/lists/*
WORKDIR /bundle/video
RUN test -n "$VERSION" || { echo "pass --build-arg VERSION=<x.y.z>" >&2; exit 1; }; \
    case "$TARGETARCH" in amd64) platform=linux-x86_64 ;; arm64) platform=linux-aarch64 ;; *) echo "no static ffmpeg for $TARGETARCH" >&2; exit 1 ;; esac; \
    sha=$(eval echo "\$FFMPEG_SHA256_$TARGETARCH"); \
    curl -fsSL -o /tmp/ffmpeg.tar.xz "https://johnvansickle.com/ffmpeg/old-releases/ffmpeg-$FFMPEG-$TARGETARCH-static.tar.xz" \
    && echo "$sha  /tmp/ffmpeg.tar.xz" | sha256sum -c - \
    && mkdir -p bin licenses \
    && tar -xJf /tmp/ffmpeg.tar.xz -C /tmp \
    && cp /tmp/ffmpeg-$FFMPEG-$TARGETARCH-static/ffmpeg /tmp/ffmpeg-$FFMPEG-$TARGETARCH-static/ffprobe bin/ \
    && cp /tmp/ffmpeg-$FFMPEG-$TARGETARCH-static/GPLv3.txt licenses/ffmpeg-GPLv3.txt \
    && printf '{"id":"video","version":"%s","contract":1,"platform":"%s","exec":"bin/fafnirx-video","env":{"FAFNIRX_FFMPEG":"{dir}/bin/ffmpeg","FAFNIRX_FFPROBE":"{dir}/bin/ffprobe","FAFNIRX_MODULE_UI":"{dir}/ui"}}' "$VERSION" "$platform" > plugin.json \
    && mkdir -p /out \
    && echo "$platform" > /tmp/platform
COPY --from=build /src/target/release/fafnirx-video bin/fafnirx-video
COPY --from=web /src/frontend/dist-modules/video ui
RUN name="fafnirx-plugin-video-$VERSION-$(cat /tmp/platform).zip" \
    && zip -qr "/out/$name" plugin.json bin ui licenses \
    && cd /out && sha256sum "$name" > "$name.sha256"

FROM scratch AS plugin-video
COPY --from=bundle-video /out /

FROM debian:bookworm-slim AS master
# rclone performs the offsite mirror uploads; ca-certificates lets it (and the master) verify remote TLS; curl is for the
# health check.
RUN apt-get update     && apt-get install -y --no-install-recommends rclone ca-certificates curl     && rm -rf /var/lib/apt/lists/*     && useradd --uid 1000 --create-home fafnirx     && mkdir /data /config && chown fafnirx /data /config
COPY --from=build /src/target/release/fafnirx-master /usr/local/bin/fafnirx-master
COPY --from=build /src/target/release/fafnirx-sign /usr/local/bin/fafnirx-sign
# /config holds rclone.conf, if you use one for mirrors. Map a host folder onto it.
ENV FAFNIRX_DATA=/data FAFNIRX_RCLONE_CONFIG=/config/rclone.conf FAFNIRX_HTTP=0.0.0.0:8080
VOLUME /data /config
EXPOSE 7443 8080
USER fafnirx
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3     CMD curl -fsS "http://127.0.0.1:${FAFNIRX_HTTP##*:}/healthz" || exit 1
CMD ["fafnirx-master"]
