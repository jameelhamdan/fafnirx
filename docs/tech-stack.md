# Tech stack

| Layer | Technology | Where / why |
|---|---|---|
| Language | Rust (edition 2021), Cargo workspace | `crates/*`. One binary per role; a cross-platform client |
| Device API | gRPC: `tonic` 0.12, `prost` 0.13 | Contract in `proto/fafnirx.proto`; streaming chunk upload. Plain HTTP/2; `https://` through a proxy uses the system trust store (`tls-roots`) |
| Admin API | `axum` 0.7 | JSON under `/api`, server-sent events, streaming downloads and zips (`crates/master/src/web` and the `api.rs` of each app) |
| Web app | React 19, TypeScript, Vite, Tailwind CSS v4, shadcn/ui (Radix) | `frontend/`; built into the master with `rust-embed` |
| Web app data | TanStack Query, TanStack Virtual, React Router | Queries keyed by kind so live events can refresh a family; virtualized file list |
| Hashing | BLAKE3 (chunks, root hash), SHA-256 (device tokens, update binaries), CRC-32 (zip) | `crates/common`, `crates/master/src/files/zip.rs` |
| Login | HMAC-SHA256 signed cookie (`hmac`, `sha2`) | `crates/master/src/web/auth.rs` |
| Update signatures | Ed25519 (`ed25519-dalek`) | `crates/sign`, verified in `crates/client/src/update.rs` |
| Async runtime | `tokio` | Master and client |
| Database | PostgreSQL 17 | Only supported database. `crates/master/schema.sql` |
| Master database access | `postgres` 0.19 (synchronous) + `r2d2` pool | Called from blocking threads; see [Development](development.md#database-access-in-the-master) |
| Cross-process coordination | PostgreSQL advisory locks, `LISTEN/NOTIFY`, `FOR UPDATE SKIP LOCKED` | Path locks, live updates, the job queue, leader election |
| Photo metadata | `kamadak-exif`, `image` | `crates/master/src/library/meta.rs`: size, EXIF, GPS, upright JPEG thumbnails and previews |
| Videos | `ffmpeg` and `ffprobe` (external programs, in the Docker image) | `crates/master/src/library/media.rs`: probing, poster frames, 720p playable copies |
| Map | Leaflet, supercluster (grouping) | `frontend/src/pages/map.tsx`; loaded only when the map or the place picker opens |
| Master file watching | `notify` | `crates/master/src/files/livetree.rs`: network drive support |
| Google Drive | Drive REST API v3 via `reqwest`, `jsonwebtoken` for the service-account JWT | `crates/master/src/integrations/gdrive.rs` |
| Mirrors | `rclone` (external binary, in the Docker image) | `crates/master/src/integrations/mirror.rs`; options passed as environment variables |
| Client file watching | `notify`, `ignore` (gitignore syntax) | `crates/client/src/scan.rs` |
| Client local state | SQLite via `rusqlite` (bundled) | The client's own upload queue and cursor. Unrelated to the master's database |
| Client credentials | OS keychain via `keyring`, file fallback | `crates/client/src/credentials.rs` |
| Client tray | `tray-icon`, `tao`, `rfd`, `resvg` | Optional feature `tray` |
| Packaging | Docker (multi-stage: Node, Rust, slim runtime), Compose; Inno Setup for the Windows client; a shell installer for Linux and macOS | `Dockerfile`, `docker-compose.yml`, `installer/` |
| Lint and format | `rustfmt`, `clippy`, `oxlint`, `tsc` | `rustfmt.toml`, `frontend/` |
| Tests | `cargo test`, Node end-to-end scripts (`pg` for the test database) | `crates/*/src`, `tests/` |

## Repository layout

```text
crates/
  common/    gRPC types (generated from proto/), chunking, hashing, path and device-name rules
  master/    fafnirx-master, organized in apps (see Development: "How the master is organized"); schema.sql
  client/    fafnirx: config, scan, hash, sync, daemon, net, update, service, credentials, tray
  sign/      fafnirx-sign: keygen, publish
proto/       fafnirx.proto
frontend/    the web app: src/pages, src/components (ui/ is shadcn), src/lib (api, live, upload)
installer/   fafnirx.iss and build.ps1 (Windows), install.sh (Linux, macOS), fafnirx-master.service (systemd)
tests/       end-to-end scripts (real master, client and fake Drive)
docs/        this documentation
```
