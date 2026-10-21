# Tech stack

| Layer | Technology | Where / why |
|---|---|---|
| Client and engine language | Rust (edition 2021), Cargo workspace | `crates/*`. One static binary per role, cross-platform client |
| Data path | gRPC over TLS: `tonic` 0.12, `prost` 0.13 | Contract in `proto/fafnirx.proto`; streaming chunk upload |
| TLS | `rustls` (via tonic), `rcgen` for the self-signed CA | The engine creates the CA and server certificate on first start; clients pin the CA |
| Hashing | BLAKE3 (chunks, root hash), SHA-256 (device tokens, update binaries) | `crates/common` |
| Update signatures | Ed25519 (`ed25519-dalek`) | `crates/sign`, verified in `crates/client/src/update.rs` |
| Async runtime | `tokio` | Engine and client |
| Database | PostgreSQL 17 | Only supported database. `schema.sql` |
| Engine database access | `postgres` 0.19 (synchronous) + `r2d2` pool | Called from `spawn_blocking`; see [Development](development.md#database-access-in-the-engine) |
| Web UI backend | Python 3.12, FastAPI, Uvicorn, Jinja2 | `web/fafnirx_web`; server-rendered templates |
| Web UI database access | `psycopg` 3 (binary) | One connection per request, dict rows |
| Web UI frontend | Hand-written CSS, Alpine.js (vendored, about 15 KB) | No build step, works offline on a LAN. Styling tokens from Beszel (MIT) and the Inter font |
| Google Drive | Drive REST API v3 via `reqwest`, `jsonwebtoken` for the service-account JWT | `crates/server/src/gdrive.rs` |
| Mirrors | `rclone` (external binary, in the Docker image) | `crates/server/src/mirror.rs`; options passed as environment variables |
| Client file watching | `notify`, `ignore` (gitignore syntax) | `crates/client/src/scan.rs` |
| Client local state | SQLite via `rusqlite` (bundled) | The client's own upload queue and cursor. Unrelated to the server database |
| Client credentials | OS keychain via `keyring`, file fallback | `crates/client/src/credentials.rs` |
| Client tray | `tray-icon`, `tao`, `rfd`, `resvg` | Optional feature `tray` |
| Packaging | Docker (multi-stage), Docker Compose | `Dockerfile`, `docker-compose.yml` |
| Lint and format | `rustfmt`, `clippy`, `ruff` | `rustfmt.toml`, `ruff.toml` |
| Tests | `cargo test`, `pytest`, Python end-to-end scripts | `tests/`, `web/tests/` |

## Repository layout

```text
crates/
  common/    gRPC types (generated from proto/), chunking, hashing, path rules
  server/    fafnirx-server: db, service (gRPC), ingest, store, jobs, sources, gdrive, mirror, tls
  client/    fafnirx: config, scan, hash, sync, daemon, net, update, service, credentials, tray
  sign/      fafnirx-sign: keygen, publish
proto/       fafnirx.proto
web/
  fafnirx_web/   app.py (routes), integrations.py (sources, mirrors), db.py, templates/, static/
  tests/         pytest
schema.sql   PostgreSQL schema, shared by engine and web UI
tests/       end-to-end scripts (real server, web UI and client)
docs/        this documentation
```
