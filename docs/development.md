# Development

## Build and run

Needs Rust ([rustup](https://rustup.rs)), Node 22+, Docker (throwaway PostgreSQL), `rclone` (mirror tests).

```sh
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
(cd frontend && npm ci)
cargo build --workspace
cargo sweep --time 1                  # drops build output unused for a day (cargo install cargo-sweep)

export FAFNIRX_DATABASE_URL=$FAFNIRX_TEST_DATABASE_URL FAFNIRX_DATA=./data FAFNIRX_ADMIN_PASSWORD=pw
cargo run -p fafnirx-master            # devices :7443, web app and API :8080
(cd frontend && npm run dev)           # hot reload :5173, proxies /api to :8080
cargo run -p fafnirx -- install --in-place --no-start --server http://127.0.0.1:7443   # token from the web app or FAFNIRX_TOKEN
```

- Building the master generates typed queries against a real PostgreSQL ([below](#typed-queries)); the client needs none.
- A checkout builds without the web app (`build.rs` writes a placeholder into `frontend/dist`); run `npm run build` in
  `frontend/` before building the master you will use.
- Client: `cargo build --profile client -p fafnirx`. Windows installer: `installer\build.ps1` (Inno Setup). Linux tray needs
  `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev` (or `--no-default-features`). `FAFNIRX_TOKEN_STORE=file` avoids the keychain.
- Images from this checkout (the data folders then appear in `docker/`): `docker compose -f docker/docker-compose.yml -f docker/docker-compose.build.yml up -d --build`.

## Test

| What | Command | Needs |
|---|---|---|
| Rust unit and API tests | `cargo test --workspace` | `FAFNIRX_TEST_DATABASE_URL` |
| Web app | `cd frontend && npx tsc -b && npx oxlint && npm run i18n -- --check` | |
| E2E: devices, uploads, watcher, renames, updates | `cd tests && npm i && node e2e.mjs` | Built binaries, test DB |
| E2E: sources and mirrors | `node e2e-io.mjs` | `rclone` (`FAFNIRX_RCLONE`) |
| E2E: video module | `node e2e-video.mjs` | `fafnirx-video` built |
| E2E: delete and retention | `node e2e-retention.mjs` | ~2 min |
| E2E: synced folders (several clients, two users) | `node e2e-folders.mjs` | Built binaries, test DB |

**Where tests live.** Never in the code they test: each crate keeps them in `<crate>/tests/unit/`, with the same path as the module (`src/library/embedded.rs` → `tests/unit/library/embedded.rs`). The module declares them with
`#[cfg(test)] #[path = "../../tests/unit/library/embedded.rs"] mod tests;`, so they stay its child and reach its private items. A new test goes in that file; a module without one gets the file and the three lines. Real integration tests of a public API are top-level files of `<crate>/tests/` (`module-api/tests/rules.rs`). Logic versus tests, in lines, is printed by `scripts/check-layout.sh`.

Database tests use a schema of their own, so they run in parallel. `crates/master/tests/unit/web/api/` drives the real router in
memory; `routes.rs` pins the access rule of every route. Video parts need ffmpeg, which tests do not: they check parsing and
the contract with the module (a stand-in module in `web/api/modules.rs`, stand-in programs in `e2e-video.mjs`). E2E scripts start the real master and client (binaries from `target/debug` or `$FAFNIRX_BIN_DIR`);
run them on a normal disk (Docker Desktop bind mounts report no file events). CI runs all of them (`npm run all`, job `e2e`).
The update test signs its stand-in release with a key of its own, which only a debug client accepts (`FAFNIRX_UPDATE_KEY`).
Unit tests never wait on the clock.

Before sending a change: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` (also `-p fafnirx --no-default-features`, the client without the tray), `cargo machete`, `cargo deny check advisories` (known vulnerabilities, `deny.toml`), `bash scripts/check-layout.sh`, `cd frontend && npx tsc -b && npx oxlint`. CI runs the same. The client's tests link only where the Windows SDK has `rc.exe`; run them in Linux (Docker) otherwise.

## Conventions

- Match the surrounding code: sparse comments that say *why*, plain names. Rust `rustfmt.toml` (calls, chains and literals longer than 100 columns break at their brackets, one item per line); TypeScript without
  semicolons, single quotes; `src/components/ui` stays as generated. UI text: short sentences, sentence case.
- A behaviour change comes with a test; no dead code; update the docs it touches.
- Log each change once and accurately: a rename is `rename`, never `delete`.
- **Layout rules** (`scripts/check-layout.sh`, CI): no test block inside a source file; no logic file over 600 lines (it becomes a folder: `mod.rs` plus files by job, children reach the private items of their parent); no function with more than five parameters (a small struct says what they are, `clippy.toml`).
- **Kinds are enums, not strings.** The database column stays text, the enum owns the spelling (`as_str`/`parse`): `RunStatus`, `MirrorKind`, `SourceKind` in `repo/links.rs`, `JobKind` and `JobStatus` in `repo/jobs.rs`, `FolderKind` in `repo/folders.rs`. A new kind is a new variant and the compiler lists every `match` to change.
- **Errors cross a boundary as a type.** The device API's handlers return `CallError` (`devices/call.rs`): a `Status`, a refusal the web app gives too, or something internal that is logged and not told. The web side has `ApiError`.

## Code layout

`crates/master/src`, one folder per app, handlers next to logic:

| App | Owns |
|---|---|
| `platform` | What apps build on: database pool, locks and `NOTIFY`, migrations, the file store, ids |
| `repo` | Storage: one module per aggregate, and the one query built at run time (the file listing) |
| `devices` | Devices: tokens, leases, the device gRPC API |
| `folders` | Folders of users and master folders, sharing, renames, totals |
| `web` | HTTP shell, logins, roles, routes and their tests |
| `files` | Content in and out: ingest (commit, delete, rename, move), the watcher, browsing, downloads, zips |
| `library` | Photos and videos: metadata jobs, pictures, artwork, albums |
| `modules` | The optional [modules](modules.md): registry, capabilities, proxy to the web app |
| `integrations` | Sources and mirrors |
| `monitoring` | Dashboard, schedules, the job queue |

New endpoints go in the app's `api.rs` and are registered in `web/mod.rs`. Apps use `platform` and each other's public
functions; `platform` knows no app. SQL lives only in `queries/*.sql` (typed, generated); no ORM.

**Links.** A source (`in`) and a mirror (`out`) are rows of one `links` table (`repo/links.rs`). The scheduler (`integrations/mod.rs`) queues a link that is due as a job (`links::schedule`; the `jobs` table is the one queue, with `meta` and `thumb`), claims queued runs (`links::claim_job`), runs each (`mirror::run`, `rclone_source::run`, `fetch_source::run`) and records how it ended: the run returns an `Outcome`, `Source(SourceRun)` or `Mirror(MirrorRun)`, each saying its own counts, and `finish` stores a `RunStatus`.

**Core vs add-ons.** Core: client, `common`, and in the master `devices`, `folders`, `files`, `monitoring`, `platform`, `repo` (minus
library), `web`, mirrors. Add-ons: the library, and the [modules](modules.md) (separate services; the master asks them for capabilities and works without them). Add-ons use the core, not the reverse; known
exceptions: `ingest.rs` queues metadata jobs; `browse.rs`/`transfer.rs` use
`library::meta`; `repo/files.rs` and `repo/jobs.rs` hold library queries; `monitoring/jobs.rs` cleans the library cache.

## Typed queries

SQL is in `queries/*.sql` as [Cornucopia](https://github.com/cornucopia-rs/cornucopia) queries (`--! name`, `:parameter`).
`crates/db/build.rs` generates typed functions into `target/` (not in the repository) against a PostgreSQL taken from
`FAFNIRX_CODEGEN_DATABASE_URL`, else `FAFNIRX_TEST_DATABASE_URL`, else a throwaway Docker/Podman container. Add a query: edit
the `.sql`, build, call it where it is needed: a plain single query straight from the caller (`sql::alerts::resolve_alert().bind(conn, &now, &kind, &key)`, with
`use fafnirx_db::queries as sql`), a query with a rule around it (several queries, a mapping, a count turned into a flag) from a `repo/` function. Only the
file listing, whose sort order the user chooses, is assembled at run time (`repo/listing.rs`; user input is never put in the text).

## Database access

Synchronous `postgres` behind an `r2d2` pool.

- Never touch the database from async code: use `db::with(&db, |conn| ...)` or `web::run`. Never create or drop the pool there.
- Helpers take `&mut impl GenericClient` (client or transaction). Integers are `i64`; ids of files, albums, tags, jobs and
  alerts are `uuid::Uuid`, and anything sent to the API uses `Id` (`platform/id.rs`).
- Changing a path: `db::lock_path` first, move files before the transaction ends, `db::notify` in the same transaction
  (`ingest::commit_version` is the model).
- A rename is `commit_version` with `renamed_from` (clients), `adopt_live` with `renamed_from` (watcher) or
  `move_path` (web app; only `user` and `master` folders). Each logs one `rename`, calls `files::vacate_renamed` (never
  `delete_path`). Albums show a whole folder, so a rename needs nothing more.
- A job returns `Ok(Some(note))` to show a note in Processing.

## Changing things

**Schema.** `crates/master/migrations/` holds the schema as numbered files. Add `NNNN_name.sql` next to them and an entry at
the end of `ALL` in `platform/migrations.rs`. Applied on start, in order, in one transaction behind an advisory lock, with a
checksum. Never edit a released migration; a newer database than the master is refused. Avoid long locks, and test on populated
data.
Update the [diagram](architecture.md#database).

**API.** Handler in the app, registered in `light()` or `heavy()` (moves file contents) in `web/mod.rs`; a response struct gets
`#[cfg_attr(test, derive(ts_rs::TS), ts(export))]`, and `cargo test -p fafnirx-master` regenerates its TypeScript type in
`frontend/src/lib/generated/` (commit it; CI checks it; `types.ts` re-exports it); a
test in `tests/unit/web/api/`; a line in `ROUTES` (`routes.rs`). Errors are `ApiError` (`bad`, `not_found`, `conflict`) →
`{ "error": "..." }`. Every handler starts from `Ctx { state, who }` (the app and the `Access` of the caller) and limits itself to the role: `check_see`/`check_write(folder)`
(others' devices answer 404), `viewer()`/`scope_args()` for file queries, `require_admin()`, `require_workspace()`. Parse ids
with `Id` and check visibility like any input. Test each role.

**Device contract.** Edit `proto/fafnirx.proto` (types regenerate). Reads are limited to client updates
and the folders the user syncs (shared, master and own); any other call that reads, lists or deletes stored data changes the security model (see
[CONTRIBUTING](../CONTRIBUTING.md)).

**A source.** Model: `integrations/rclone_source.rs` (a file is known by its path). A source is a `links` row (direction `in`,
shared with mirrors: `repo/links.rs`) whose `folder_id` is the folder it writes into (from the `folder` of the request, checked by
`integrations/api/source.rs`; a source without one gets a folder `source-<name>`). The scheduler queues it when due and
dispatches by `kind`. A source lists its files as
`syncsource::Item`s (path, optional key, marker) and fetches one; `syncsource::apply` compares the listing with its `link_items` (by
key, else path) and the live `files` of the folder, moves a file whose key has a new name (`ingest::move_path`), downloads the
rest to temp files, records with `ingest::commit_version` and `mark_deleted`, and deletes nothing if listing fails. A kind of
module also offers `load` in the SDK so the Edit form can show its settings.

## The web app

Vite, React 19, TypeScript, Tailwind v4, shadcn/ui, TanStack Query, React Router.

- `src/lib/api.ts` is the only JSON `fetch`; ids are strings. `src/lib/live.tsx` invalidates query families by SSE topic.
  `src/lib/upload.ts` is the upload queue. Pages load lazily; the file list is virtualized.
- `/watch/:id` (`pages/watch.tsx`) is the standalone viewer; the Files sheet reuses `InfoPanel` (`embedded`) and `WorkNote`.
- **Translations:** write every visible string in English in `t('…')` (`useT()`; `msg('…')` for strings in data shown with `t`
  later; `tr('…')` outside components); `{name}` placeholders come from the second argument. `npm run i18n` updates
  `translations/en.json` and lists gaps; `-- --check` fails if stale or incomplete. A language is `translations/<code>.json` plus
  a line in `LANGUAGES` (`src/lib/i18n.ts`); Arabic is RTL, so use `start`/`end`, not `left`/`right`.
- **Server messages** are English and translated when they arrive (`translateServer` in `src/lib/i18n.ts`, called by `lib/api.ts`): `npm run i18n`
  also collects the literal and `format!` messages of `ApiError::bad/not_found/conflict`, `(StatusCode::…, "…")`, `forbidden`
  and `*_RULE` in `crates/master/src`. Name every `format!` part (`{name}`, not `{}`): it is the placeholder.
- **Modules:** `frontend/src/modules/<id>` is built apart (`npm run build:module -- <id>`, into `dist-modules/`) and joins the app
  only through `src/sdk` (checked by `npx oxlint`); the video module is `modules/video` (its player, `player/`, imports nothing
  from the app) with its backend in `modules/video/service`. To try one in development, build it, run its service with
  `FAFNIRX_PLUGIN_UI=frontend/dist-modules/<id>`, and have it announce itself to the master (`FAFNIRX_MASTER_URL`, `FAFNIRX_PLUGIN_URL`, `FAFNIRX_PLUGIN_SECRET`). Keep both
  READMEs true. Adding one: [modules](modules.md#writing-one).

## Releasing

Actions → **Release** → Run workflow on `master` with `X.Y.Z` (*dry_run* rehearses, *homebrew* updates the formula). No manual version change.

1. **Check:** version is `X.Y.Z`, newer than all releases, unused, not below `Cargo.toml`; publishing secrets exist; Docker Hub login works.
2. **Version commit** by `prepare-release.sh` (`Cargo.toml`, `Cargo.lock`) on branch `release/X.Y.Z`; every later job tests that commit.
3. **Verify:** CI; client builds for Windows, macOS, Linux (installer, `.deb`, and the plain `fafnirx-X.Y.Z-<os>-<arch>` + `.sha256` clients update from, the checksums signed before publishing when `UPDATE_SIGNING_KEY` is set); Docker image started on amd64 and arm64.
4. **Publish, only if all passed:** fast-forward `master`, push the image (`<version>`, `<major>.<minor>`, `latest`), tag and GitHub Release, then Homebrew. `release/X.Y.Z` stays at the released commit (check it out to go back to that version, or branch from it for a fix); a run that did not publish deletes it.

A failure before step 4 leaves `master` untouched; if `master` moved, nothing is published: run again. If publishing fails
halfway, run again with the same version. Secrets (repository, not environment): `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` (access
token), `HOMEBREW_TAP_TOKEN`; optional `UPDATE_SIGNING_KEY` (Ed25519 private key, PEM; step 1 checks it is the one in
`crates/client/src/update/release.rs`, whose clients refuse a signature by another key); optional variable `DOCKERHUB_IMAGE`. The bot must be able to push to a protected `master`; add
reviewers to the `release` environment for a manual gate. `.github/scripts/smoke-image.sh <image>` runs the image check.

## Troubleshooting

`set FAFNIRX_TEST_DATABASE_URL`: start the test PostgreSQL and export it. `Cannot start a runtime from within a runtime`: a
pool was made or dropped in async code. Port 8080 busy: set `FAFNIRX_HTTP`.
