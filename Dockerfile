# One image for both orchestrator processes: the Rust engine (fafnirx-server)
# and the Python web UI (python -m fafnirx_web). See docker-compose.yml.

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY proto proto
COPY schema.sql schema.sql
RUN cargo build --release -p fafnirx-server -p fafnirx-sign

FROM python:3.12-slim
# rclone performs the offsite mirror uploads; ca-certificates lets it (and the engine) verify remote TLS.
RUN apt-get update     && apt-get install -y --no-install-recommends rclone ca-certificates     && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY web/requirements.txt /app/web/requirements.txt
RUN pip install --no-cache-dir -r /app/web/requirements.txt
COPY web /app/web
COPY schema.sql /app/schema.sql
COPY --from=build /src/target/release/fafnirx-server /usr/local/bin/fafnirx-server
COPY --from=build /src/target/release/fafnirx-sign /usr/local/bin/fafnirx-sign
WORKDIR /app/web
ENV FAFNIRX_DATA=/data \
    PYTHONUNBUFFERED=1
VOLUME /data
EXPOSE 7443 8080
CMD ["fafnirx-server"]
