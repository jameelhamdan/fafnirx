# Development

## Setup

You need the [Rust toolchain](https://rustup.rs), Node 22 or newer (for the web app and the end-to-end tests),
Docker (for a throwaway PostgreSQL), and `rclone` for the mirror tests.

```sh
# A PostgreSQL for tests
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres

(cd frontend && npm ci)        # the web app
cargo build --workspace
```

A fresh checkout builds without the web app: `crates/master/build.rs` writes a placeholder page into
`frontend/dist` when there is none. Build the app (`npm run build` in `frontend/`) before building the master you will use.

## Run the pieces by hand

```sh
export FAFNIRX_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
export FAFNIRX_DATA=./data FAFNIRX_ADMIN_PASSWORD=pw
cargo run -p fafnirx-master                  # device API on :7443, web app and admin API on :8080

cd frontend && npm run dev                   # the web app with hot reload on :5173, proxying /api to :8080
```

Create a device in the web app, then `cargo run -p fafnirx -- enroll --no-service --server http://127.0.0.1:7443` and paste the token (or set `FAFNIRX_TOKEN`).
Set `FAFNIRX_TOKEN_STORE=file` to avoid the OS keychain while developing.

## Tests

| What | Command | Needs |
|---|---|---|
| Rust unit and API tests | `cargo test --workspace` | `FAFNIRX_TEST_DATABASE_URL` |
| Web app types and lint | `cd frontend && npx tsc -b && npx oxlint` | |
| End to end: devices, uploads, watcher, resume, updates | `cd tests && npm i && node e2e.mjs` | Built binaries, test database |
| End to end: the roles running apart | `node e2e-roles.mjs` | Same |
| End to end: Drive (fake API) and mirrors | `node e2e-io.mjs` | Also `rclone` (`FAFNIRX_RCLONE`) |
| End to end: delete and retention | `node e2e-retention.mjs` | About 2 minutes |

- Database-backed tests create a schema of their own on the test server and drop it afterwards, so they run in parallel.
- `crates/master/src/web/tests.rs` drives the real admin router in memory: login, devices, chunked upload, browsing,
  range downloads, zip, history, restore and delete, and the photo library (timeline, edits, tags, albums, trash, map). The video parts need `ffmpeg`, which the tests do not: they check the parsing
  of `ffprobe` output and the job lanes instead.
- The end-to-end scripts start the real `fafnirx-master`, the real client binary and a fake Google Drive. They look for the
  binaries in `target/debug`, or in `$FAFNIRX_BIN_DIR` (useful when `CARGO_TARGET_DIR` is elsewhere). They share
  `tests/lib.mjs`.
- The file watcher tests rely on file events. On a mount that reports none (a Docker Desktop bind mount) the master falls
  back to comparing the tree every 5 minutes, so run the end-to-end scripts on a normal disk.

Before sending a change:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
cd frontend && npx tsc -b && npx oxlint
```

## Conventions

- Match the surrounding code: comments are sparse and explain *why*; names are plain.
- Rust: `rustfmt.toml` sets `max_width = 120`. TypeScript: no semicolons, single quotes, the shadcn/ui components in
  `src/components/ui` stay as generated.
- The web app's wording is short sentences in sentence case; the product name stays `fafnirx`.
- A change to behaviour comes with a test: a unit test where the logic is isolated, an end-to-end check when it spans
  processes.
- No dead code: remove what a change makes unused in the same change.

## How the master is organized

`crates/master/src` is split into apps, each folder owns one part of the domain and
holds its API handlers next to its logic, instead of one folder per kind of file.

| App | Owns | Files |
|---|---|---|
| `platform` | What every app builds on | `db.rs` (pool, schema, locks, `NOTIFY`, settings, alerts), `store.rs` (the file store on disk) |
| `devices` | Clients and their tokens | `grpc.rs` (the device-facing API, upload only), `api.rs` (admin: list, add, rotate, revoke, settings) |
| `files` | Content going in and coming out | `ingest.rs` (commit versions), `livetree.rs` (watcher), `browse.rs` (file browser), `transfer.rs` (downloads, uploads), `zip.rs` |
| `library` | Photos and videos | `meta.rs` (metadata, previews, job queue), `media.rs` (ffmpeg), `photos.rs` (timeline, map, trash, edits), `albums.rs` |
| `integrations` | Sources and mirrors | `mod.rs` (scheduler), `gdrive.rs`, `mirror.rs`, `api.rs` |
| `monitoring` | Watching the system | `api.rs` (dashboard, alerts), `jobs.rs` (scheduled alerts, retention) |
| `web` | The HTTP shell | `mod.rs` (state, errors, the route table), `auth.rs`, `events.rs` (live updates), `assets.rs` (the embedded web app), `tests.rs` |

Rules of thumb: a new endpoint goes in the `api.rs` of its app and is registered in `web/mod.rs`; apps may use
`platform` freely and each other's public functions, but `platform` knows nothing of the apps. The SQL is written
next to the code that uses it with the `postgres` crate; there is no ORM (see [Database access](#database-access-in-the-master)).

## Database access in the master

The master uses the synchronous `postgres` crate behind an `r2d2` pool (`crates/master/src/platform/db.rs`). Rules that follow:

- **Never touch the database from async code.** `postgres` drives its own runtime and panics inside Tokio. Use
  `db::with(&db, |conn| ...)` (or `web::run` in a handler), which runs the closure on a blocking thread with a pooled
  `postgres::Client`.
- **Never create or drop a `Db` (the pool) on an async thread.** `main` leaks one reference on purpose, and tests that
  need the runtime build their state first and call `block_on` afterwards.
- Helpers take `&mut impl GenericClient`, so they work on both a `Client` and a `Transaction`.
- Parameters are `$1, $2`; integers are `i64` (every integer column is `BIGINT`).
- Use `INSERT ... RETURNING id` instead of "last insert id".
- Anything that changes one path takes `db::lock_path` first and moves files before the transaction ends
  (`ingest::commit_version` is the model). Announce changes with `db::notify` inside the same transaction.

Unit tests get a fresh schema from `db::testing::db()`.

## Changing the schema

The schema is a list of numbered migrations in `crates/master/migrations/` (`0001_initial.sql`, `0002_....sql`), listed in
`crates/master/src/platform/migrations.rs`. Every master process applies the ones it has not applied yet when it starts,
in order, in one transaction behind an advisory lock, and records each one in `schema_migrations` with a checksum. A new
database gets all of them; an existing one only the new ones. To change the schema:

1. Add the next numbered `.sql` file and an entry at the end of `ALL`. A migration runs once, so it can be plain
   `ALTER TABLE ...`; it must work while older processes are still running, because masters restart one by one.
2. Never edit a migration that has been released: the checksum no longer matches and the master refuses to start.
3. A database that is newer than the master (a downgrade) is refused with a message.
4. Update [the database diagram](architecture.md#database-structure) and the admin API types in `frontend/src/lib/types.ts`.

## Changing the admin API

Add the handler in the `api.rs` (or, for files, `browse.rs` or `transfer.rs`) of the app it belongs to, register it in `light()` or `heavy()` in `crates/master/src/web/mod.rs` (heavy = moves file
contents), add the type to `frontend/src/lib/types.ts`, and cover it in `web/tests.rs`. Errors are `ApiError`
(`bad`, `not_found`, `conflict`) and reach the browser as `{ "error": "..." }`.

## Changing the device gRPC contract

Edit `proto/fafnirx.proto`; `crates/common/build.rs` regenerates the Rust types. Keep it additive: old clients stay in
the field for a long time because they update themselves only to signed builds. Preserve the one-way rule: do not add
any call that reads, lists or deletes stored data.

## Adding a source

Sources are modules in `crates/master/src/integrations`: `gdrive.rs` is the model. A source:

1. Gets a `sources` row (kind, JSON config) and a `source-<name>` device row; `integrations/api.rs` creates both.
2. Is claimed by `integrations::claim_source`, then dispatched by `kind` in `integrations::run_source`.
3. Plans work by comparing the remote listing with `source_items` and the live `files`, downloads files to temporary files in
   the `Store`, and records versions through `ingest::commit_version` and `ingest::mark_deleted`.
4. Never deletes anything when listing the remote fails.

## The web app

`frontend/` is Vite, React 19, TypeScript, Tailwind v4 and shadcn/ui, with TanStack Query for data and React Router.

- `src/lib/api.ts` is the only place that calls `fetch` for JSON; it adds the marker header and turns errors into `ApiError`.
- `src/lib/live.tsx` listens to `/api/events` and refreshes queries by topic. Queries are keyed by kind first
  (`['files', ...]`, `['device', name]`) so a topic can invalidate a whole family.
- `src/lib/upload.ts` is the upload queue (chunks, resume, three at a time).
- Pages are loaded lazily; the file list is virtualized.

## Troubleshooting a dev setup

- `set FAFNIRX_TEST_DATABASE_URL ...`: start the test PostgreSQL (above) and export the variable.
- `Cannot start a runtime from within a runtime`: a pool was created or dropped inside async code (see above).
- Port `8080` already in use: another master (or an old container) is running; set `FAFNIRX_HTTP`.
