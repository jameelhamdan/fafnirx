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

## Building the client and the installers

```sh
cargo build --profile client -p fafnirx              # target/client/fafnirx (the profile the releases use)
sh installer/install.sh --server http://nas.lan:7443 --folder ~/Documents      # Linux, macOS: uses that binary
installer\build.ps1                              # Windows: dist\fafnirx-setup-<version>.exe (needs Inno Setup)
```

On Linux the tray icon needs the GTK and appindicator development packages (`libgtk-3-dev libayatana-appindicator3-dev
libxdo-dev`); build with `--no-default-features` to leave it out. To build the master image from a checkout:
`docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build`.

## Run the pieces by hand

```sh
export FAFNIRX_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
export FAFNIRX_DATA=./data FAFNIRX_ADMIN_PASSWORD=pw
cargo run -p fafnirx-master                  # device API on :7443, web app and web API on :8080

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
| End to end: Drive (fake API) and mirrors | `node e2e-io.mjs` | Also `rclone` (`FAFNIRX_RCLONE`) |
| End to end: video streaming through the master (stand-in ffmpeg) | `node e2e-stream.mjs` | `fafnirx-stream` built (`cargo build --workspace`) |
| End to end: delete and retention | `node e2e-retention.mjs` | About 2 minutes |

- Database-backed tests create a schema of their own on the test server and drop it afterwards, so they run in parallel.
- `crates/master/src/web/tests/` (one file per area: auth, devices, files, library, users, and `routes.rs`, which pins the access rule of every route) drives the real router in memory: login, users and roles, devices, chunked upload, browsing,
  range downloads, zip, history, restore and delete, and the photo library (timeline, edits, tags, albums, trash, map). The video parts need `ffmpeg`, which the tests do not: they check the parsing
  of `ffprobe` output and the job lanes instead.
- The end-to-end scripts start the real `fafnirx-master`, the real client binary and a fake Google Drive. They look for the
  binaries in `target/debug`, or in `$FAFNIRX_BIN_DIR` (useful when `CARGO_TARGET_DIR` is elsewhere). They share
  `tests/lib.mjs`.
- Unit tests never wait for the clock or for the operating system: time is paused or compared with hours of margin, and the
  watcher is tested through its two halves (which device and path an event stands for, and comparing a folder with the
  database). The end-to-end scripts check the real watcher by polling. On a mount that reports no file events (a Docker
  Desktop bind mount) the master compares the tree every 5 minutes instead, so run them on a normal disk.

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
| `devices` | Clients and their tokens | `grpc.rs` (the device-facing API, upload only), `api.rs` (list, add, rotate, revoke, settings) |
| `web` (users) | Logins, roles and what each reaches | `auth.rs` (login, sessions, passwords), `access.rs` (the rules: which devices a role can see and change), `users.rs` (the users API) |
| `files` | Content going in and coming out | `ingest.rs` (commit versions), `livetree.rs` (watcher), `browse.rs` (file browser), `transfer.rs` (downloads, uploads), `zip.rs` |
| `library` | Photos and videos | `meta.rs` (metadata, previews, job queue), `media.rs` (ffmpeg), `photos.rs` (timeline, map, trash, edits), `albums.rs` |
| `integrations` | Sources and mirrors | `mod.rs` (scheduler), `gdrive.rs`, `mirror.rs`, `api.rs` |
| `monitoring` | Watching the system | `api.rs` (dashboard, alerts), `jobs.rs` (scheduled alerts, retention) |
| `web` | The HTTP shell | `mod.rs` (state, errors, the route table), `auth.rs`, `events.rs` (live updates), `assets.rs` (the embedded web app), `tests/` |

Rules of thumb: a new endpoint goes in the `api.rs` of its app and is registered in `web/mod.rs`; apps may use
`platform` freely and each other's public functions, but `platform` knows nothing of the apps. The SQL is written
next to the code that uses it with the `postgres` crate; there is no ORM (see [Database access](#database-access-in-the-master)).

## Typed queries (Cornucopia)

SQL lives in `queries/*.sql`, one file per area, written as [Cornucopia](https://github.com/cornucopia-rs/cornucopia)
queries (`--! name`, named `:parameters`). `scripts/codegen.sh` (or `codegen.ps1`) turns them into the crate
`crates/db` (`fafnirx-db`): a typed function and row struct per query, checked against the real schema, which is the
migrations in `crates/master/migrations`. The generated crate is committed, so building needs no database; CI
regenerates it and fails on a difference. To add or change a query:

1. Edit or add the `.sql` file in `queries/`.
2. Start a PostgreSQL (the test one will do) and run `DATABASE_URL=postgresql://postgres:test@localhost:55432
   scripts/codegen.sh`. Install the CLI once with `cargo install cornucopia --locked`.
3. Call it from a repository function: `fafnirx_db::queries::alerts::resolve_alert().bind(&mut Pg(conn), &now, &kind, &key)`.
   `platform::pg::Pg` lets generated queries run on any `postgres` client or transaction.

New SQL goes here, not inline in handlers. Queries that are built at run time (for example the folder listing with its
sort and filter) stay as explicit SQL in a repository function until they can be expressed as fixed queries.

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
`crates/master/src/platform/migrations.rs`. The master applies the ones it has not applied yet when it starts,
in order, in one transaction behind an advisory lock, and records each one in `schema_migrations` with a checksum. A new
database gets all of them; an existing one only the new ones. To change the schema:

1. Add the next numbered `.sql` file and an entry at the end of `ALL`. A migration runs once, so it can be plain
   `ALTER TABLE ...`; it should not take long locks on big tables, because the master waits for it before it starts.
2. Never edit a migration that has been released: the checksum no longer matches and the master refuses to start.3. A database that is newer than the master (a downgrade) is refused with a message.
4. Update [the database diagram](architecture.md#database-structure) and the web API types in `frontend/src/lib/types.ts`.

## Changing the API

Add the handler in the `api.rs` (or, for files, `browse.rs` or `transfer.rs`) of the app it belongs to, register it in `light()` or `heavy()` in `crates/master/src/web/mod.rs` (heavy = moves file
contents), add the type to `frontend/src/lib/types.ts`, and cover it in `web/tests/`. Errors are `ApiError`
(`bad`, `not_found`, `conflict`) and reach the browser as `{ "error": "..." }`.

Every handler under `/api` gets the user as `Extension<Access>` (the guard in `web/mod.rs` puts it there) and must limit what it
reads and changes to what that user's role reaches: `access.check_see(folder)` / `check_write(folder)` for one device (a device
the user may not see answers 404), `access::scope` or `Access::visible_sql()` in a query over files, `require_admin()` for what is
for administrators, `require_workspace()` for what read-only users do not have. Add a case for each role in `web/tests/`, and a line for the route in `ROUTES` (`tests/routes.rs`).

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
- **Translations.** The web app is available in English, Spanish, Chinese (simplified), Arabic, Russian and Japanese. Every visible string is written in English inside `t('…')` (`const t = useT()` in a component;
  `msg('…')` marks a string in data that is shown with `t(text)` later; `tr('…')` is `t` for code outside components,
  as in `lib/format.ts`). `{name}` placeholders are filled from the second argument. `npm run i18n` collects the calls
  into `translations/en.json` and lists what the other languages lack; `npm run i18n -- --check` fails when `en.json` is
  out of date or a language is incomplete. A language is a file `translations/<code>.json` plus a line in `LANGUAGES`
  (`src/lib/i18n.ts`); the language is the saved choice, else the browser's, else English, and the page direction follows
  it (`dir="rtl"` for Arabic, so use `start`/`end` utilities, not `left`/`right`). A string without a translation shows
  in English.

## Releasing

`master` is the development branch: every branch is merged there, and CI runs on it. A release is one run of one workflow,
and nothing has to be done by hand before it, not even changing the version number.

Actions -> **Release** -> Run workflow, on `master`, with the new version as `X.Y.Z` (no suffix). Tick *dry_run* to try it
without publishing, and *homebrew* to also update the Homebrew formula (off by default).

What it does, in order:

1. **Checks the version:** it must be `X.Y.Z`, newer than every earlier release, not used before, and not below the one in
   `Cargo.toml`. It also checks that the secrets of the publishing steps are set and that Docker Hub accepts the login, so
   that a missing secret stops the run now and not after `master` has been changed.
2. **Makes the version commit.** `.github/scripts/prepare-release.sh` sets the version in `Cargo.toml` (`[workspace.package]`)
   and for the workspace's crates in `Cargo.lock`, commits it as "Release X.Y.Z" by the `github-actions[bot]`, and pushes it
   to the temporary branch `release/X.Y.Z`. From here on every job builds and tests *that exact commit*, so what is tested is
   what is released. If `Cargo.toml` already says that version, there is nothing to commit.
3. **Checks everything:** CI, the client for Windows, macOS and Linux (`.exe`, tarballs, `.deb`, each with a `.sha256`), and
   the Docker image built and started on amd64 and arm64.
4. **Publishes, only when all of that has passed:** the version commit becomes the head of `master` (a fast-forward), the image
   goes to Docker Hub (`<version>`, `<major>.<minor>`, `latest`), then the tag and the GitHub Release with the client files,
   then Homebrew. The temporary branch is deleted at the end, whatever happened.

A run that fails before step 4 leaves nothing behind and `master` is untouched. If `master` has moved on while the checks
ran, the version commit cannot be added to it: nothing is published, run the workflow again. If publishing fails halfway,
run it again with the same version: the version commit is on `master` by then, so the rerun has nothing to commit and
releases `master` as it is.

One-time setup, in the repository's Settings -> Secrets and variables -> Actions, as **repository secrets** with exactly
these names: `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` (an access token with write rights, made in Docker Hub under
Account settings -> Personal access tokens; not the account password), and `HOMEBREW_TAP_TOKEN` if you use Homebrew.
Optionally the *variable* `DOCKERHUB_IMAGE`. Secrets kept in an environment, in Dependabot's list or as variables are
not seen by the publishing jobs. The workflow pushes to `master` as the
`github-actions[bot]`, so if `master` is protected, that bot must be allowed to push there. To ask for a click before
publishing, add required reviewers to the `release` environment (Settings -> Environments). The server is installed from the
Docker image only, so there is no server package. `.github/scripts/smoke-image.sh <image>` runs the image check by hand, and
CI runs the version bump on every push, so that it cannot break unnoticed.

## The player and the streaming service

The video player is a module of its own, `frontend/src/player`: it imports nothing from the app, and the app joins it in
`components/app-video.tsx` (language, messages) and `lib/player-source.ts` (the addresses for a library video or a file in
the browser). Its backend is `crates/stream`, a service that knows nothing of users or databases. Each has a README
with its contract (the props and the HTTP API), so either can be lifted out and released on its own: keep those two
documents true when changing either side. What `ffprobe` says about a video is read by `fafnirx_common::probe`, shared
by the master and the service.

## Troubleshooting a dev setup

- `set FAFNIRX_TEST_DATABASE_URL ...`: start the test PostgreSQL (above) and export the variable.
- `Cannot start a runtime from within a runtime`: a pool was created or dropped inside async code (see above).
- Port `8080` already in use: another master (or an old container) is running; set `FAFNIRX_HTTP`.
