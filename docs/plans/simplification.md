# Plan: simplifying fafnirx

The goal is a simple, reliable, fast, easy-to-use service for file backups. The core is small: the client sync engine,
the master's ingest, versioning and retention, users and roles, the file browser, and mirrors, installed with Docker.
Everything else is a feature on top of that core, and each one costs code, tests, translations, documentation and
failure modes. This list ranks them. Nothing here has been removed; each line is a decision for the owner.

Sizes are lines of source, counted from the repository.

## Candidates to remove or split out

| Feature | Where | Size | What removing it saves | Risk | Recommendation |
|---|---|---|---|---|---|
| Photo library (timeline, map, albums, tags, ratings, trash) | `master/src/library` (photos, albums, meta, media: about 2,400), 6 frontend pages and `components/library/*`, migrations 0002-0008 parts | large | The EXIF/thumbnail pipeline, `image` and ffprobe use, a big share of the web app and tests | Users who browse photos lose it; the file browser still previews photos | **Split out**: keep the file browser with previews, drop timeline/map/tags/albums unless photo browsing is a goal of the product |
| Video streaming service and player | `crates/stream` (about 1,700), `library/stream.rs`, `frontend/src/player`, `docs/streaming.md`, compose `stream` profile | large | A second binary, ffmpeg in the image, HLS code, subtitle code, the player | Anyone who watches videos from the master | **Remove** if the product is backup; the file browser can download or play a file directly in the browser |
| Google Drive sources | `integrations/gdrive.rs` (about 630), source parts of `integrations/*`, sources page | medium | Service-account handling, a polling job, retention interplay (`source_items`) | Users who pull Drive into the master | **Remove** or keep as a documented "run rclone yourself" recipe: rclone mirrors in the other direction are the supported route |
| Five translations next to English | `frontend/translations/*.json`, `scripts/extract-i18n.mjs` | 5 x 494 lines | Keeping 6 files in step with every UI change | Non-English users | **Keep only if** they are maintained; otherwise ship English and take contributions later |
| Self-update with signed builds | `crates/sign`, `client/src/update.rs`, `get_update_info`/`download_update` RPCs, `docs/usage.md#signed-client-updates` | medium | A key to manage, an update channel in the server, two RPCs, the only server-to-device payload | Devices need a manual reinstall for each version | **Keep** while the installers are not on package managers: it is what makes upgrades of many devices easy; revisit once `.deb` and Homebrew are published |
| Tray icon | `client/src/tray`, GTK/appindicator dependencies on Linux | about 1,000 | The slowest and most fragile build step, GTK packages, an optional feature already | Users who want a visual status | **Keep as optional** (already a cargo feature); never required to run the client |
| Reverse proxy and Cloudflare tunnel recipes | `Caddyfile`, `docker-compose.proxy.yml`, `docs/deployment.md` | small | One compose overlay and a doc section | Remote access users | **Keep**: it is what makes https for clients possible |
| Network drive writes into `files/` (live tree watcher) | `master/src/files/livetree.rs` (about 570) | medium | A watcher with a periodic full compare, adoption logic, races with ingest | NAS-style users | **Review**: it is the biggest source of races (the master noticing files it did not write). If sharing is read-only, the watcher can go and local uploads stay web-only |

## Simplifications that keep every feature

- One way to install the master (done: image from Docker Hub, `docker-compose.yml` and `.env`; the build overlay is for
  developers only).
- Release assets are the same names on every platform; a one-line client installer that picks the right tarball and
  checks its `.sha256` would remove most of the per-platform steps in the installation guide.
- Client config has few options; `watch`, `scan_interval_secs` and the bandwidth limit are enough.
  `web_url` only serves the tray and could be derived always.
- `master/src/web/tests.rs` (1,300 lines) holds the API tests in one file; split it per area (auth, files, library,
  users) when it is next touched. It now also pins the access rule of every route (`ROUTES`), so a route cannot be added
  without deciding who may call it.
- The master runs the API, transfers and background workers in one binary with "roles"; if nobody runs them split,
  drop the role switch.

## Not recommended

- Changing the database or the file layout: plain files under `files/<user>/<device>/` are the product's best property
  (restore with `cp`, mirror with rclone, share with SMB).
- Two-way sync or restoring to a device: it would break the read-only rule for clients.

## Suggested order

1. Decide whether this is a **backup service** or a **photo and media server**. Everything marked above follows from
   that answer.
2. If backup: remove streaming, then the Drive sources, then the photo library, in that order (largest independent
   piece first). Each removal is one commit with its migration (a new migration that drops the tables, never an edit
   of a released one), docs and translation keys.
3. Then trim translations and the live tree watcher as decided.
