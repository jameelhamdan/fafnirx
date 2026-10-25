# Development

## Build and run

Needs Rust ([rustup](https://rustup.rs)), Node 22+, Docker (throwaway PostgreSQL), `rclone` (mirror tests).

```sh
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
(cd frontend && npm ci)
cargo build --workspace

export FAFNIRX_DATABASE_URL=$FAFNIRX_TEST_DATABASE_URL FAFNIRX_DATA=./data FAFNIRX_ADMIN_PASSWORD=pw
cargo run -p fafnirx-master            # devices :7443, web app and API :8080
(cd frontend && npm run dev)           # hot reload :5173, proxies /api to :8080
cargo run -p fafnirx -- enroll --no-service --server http://127.0.0.1:7443   # token from the web app or FAFNIRX_TOKEN
```

- Building the master generates typed queries against a real PostgreSQL ([below](#typed-queries)); the client needs none.
- A checkout builds without the web app (`build.rs` writes a placeholder into `frontend/dist`); run `npm run build` in
  `frontend/` before building the master you will use.
- Client: `cargo build --profile client -p fafnirx`. Windows installer: `installer\build.ps1` (Inno Setup). Linux tray needs
  `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev` (or `--no-default-features`). `FAFNIRX_TOKEN_STORE=file` avoids the keychain.
- Master image: `docker compose -f docker-compose.yml -f docker-compose.build.yml up -d --build`.

## Test

| What | Command | Needs |
|---|---|---|
| Rust unit and API tests | `cargo test --workspace` | `FAFNIRX_TEST_DATABASE_URL` |
| Web app | `cd frontend && npx tsc -b && npx oxlint && npm run i18n -- --check` | |
| E2E: devices, uploads, watcher, renames, updates | `cd tests && npm i && node e2e.mjs` | Built binaries, test DB |
| E2E: Drive (fake) and mirrors | `node e2e-io.mjs` | `rclone` (`FAFNIRX_RCLONE`) |
| E2E: streaming | `node e2e-stream.mjs` | `fafnirx-stream` built |
| E2E: delete and retention | `node e2e-retention.mjs` | ~2 min |

Database tests use a schema of their own, so they run in parallel. `crates/master/src/web/tests/` drives the real router in
memory; `routes.rs` pins the access rule of every route. Video parts need ffmpeg, which tests do not: they check parsing and
job lanes. E2E scripts start the real master, client and a fake Drive (binaries from `target/debug` or `$FAFNIRX_BIN_DIR`);
run them on a normal disk (Docker Desktop bind mounts report no file events). Unit tests never wait on the clock.

Before sending a change: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cd frontend && npx tsc -b && npx oxlint`.

## Conventions

- Match the surrounding code: sparse comments that say *why*, plain names. Rust `max_width = 120`; TypeScript without
  semicolons, single quotes; `src/components/ui` stays as generated. UI text: short sentences, sentence case.
- A behaviour change comes with a test; no dead code; update the docs it touches.
- Log each change once and accurately: a rename is `rename`, never `delete`.

## Code layout

`crates/master/src`, one folder per app, handlers next to logic:

| App | Owns | Files |
|---|---|---|
| `platform` | What apps build on | `db.rs` (pool, locks, `NOTIFY`), `migrations.rs`, `pg.rs`, `store.rs`, `id.rs` (UUID ↔ short id), `proxy.rs` |
| `repo` | Storage; the only code that knows tables | One module per aggregate, and `listing.rs` (the one query built at run time) |
| `devices` | Clients, tokens | `grpc.rs` (upload-only), `api.rs`, `totals.rs` |
| `web` | HTTP shell, logins, roles | `mod.rs` (state, errors, routes), `auth.rs`, `access.rs`, `users.rs`, `events.rs`, `assets.rs`, `tests/` |
| `files` | Content in and out | `ingest.rs` (commit, delete, rename, move), `livetree.rs`, `browse.rs`, `transfer.rs`, `zip.rs` |
| `library` | Photos and videos | `meta.rs` (jobs, pictures), `media.rs` (ffmpeg), `artwork.rs`, `photos.rs`, `albums.rs`, `stream.rs` |
| `integrations` | Sources, mirrors | `mod.rs`, `gdrive.rs`, `mirror.rs`, `api.rs` |
| `monitoring` | Dashboard, schedules, queue | `api.rs`, `jobs.rs`, `queue.rs` |

New endpoints go in the app's `api.rs` and are registered in `web/mod.rs`. Apps use `platform` and each other's public
functions; `platform` knows no app. SQL lives only in `queries/*.sql` and `repo/`. No ORM.

**Core vs add-ons.** Core: client, `common`, and in the master `devices`, `files`, `monitoring`, `platform`, `repo` (minus
library), `web`, mirrors. Add-ons: the library, streaming, Drive sources. Add-ons use the core, not the reverse; known
exceptions: `ingest.rs` queues metadata jobs and calls `repo::albums::follow_folder_move`; `browse.rs`/`transfer.rs` use
`library::meta`; `repo/files.rs` and `repo/jobs.rs` hold library queries; `monitoring/jobs.rs` cleans the library cache.

## Typed queries

SQL is in `queries/*.sql` as [Cornucopia](https://github.com/cornucopia-rs/cornucopia) queries (`--! name`, `:parameter`).
`crates/db/build.rs` generates typed functions into `target/` (not in the repository) against a PostgreSQL taken from
`FAFNIRX_CODEGEN_DATABASE_URL`, else `FAFNIRX_TEST_DATABASE_URL`, else a throwaway Docker/Podman container. Add a query: edit
the `.sql`, build, call it from a `repo/` function (`q::resolve_alert().bind(&mut Pg(conn), &now, &kind, &key)`). Only the
file listing, whose sort order the user chooses, is assembled at run time (`repo/listing.rs`; user input is never put in the text).

## Database access

Synchronous `postgres` behind an `r2d2` pool.

- Never touch the database from async code: use `db::with(&db, |conn| ...)` or `web::run`. Never create or drop the pool there.
- Helpers take `&mut impl GenericClient` (client or transaction). Integers are `i64`; ids of files, albums, tags, jobs and
  alerts are `uuid::Uuid`, and anything sent to the API uses `Id` (`platform/id.rs`).
- Changing a path: `db::lock_path` first, move files before the transaction ends, `db::notify` in the same transaction
  (`ingest::commit_version` is the model).
- A rename is `commit_version` with `renamed_from` (clients, sources), `adopt_live` with `renamed_from` (watcher) or
  `move_path` (web app; only `local` and `master` devices). Each logs one `rename`, calls `files::vacate_renamed` (never
  `delete_path`) and `albums::follow_folder_move`.
- A job returns `Ok(Some(note))` to show a note in Processing.

## Changing things

**Schema.** Add `NNNN_name.sql` to `crates/master/migrations/` and an entry at the end of `ALL` in `platform/migrations.rs`.
Applied on start, in order, in one transaction behind an advisory lock, with a checksum. Never edit a released migration; a
newer database than the master is refused. Avoid long locks. Test on populated data (see `ids_become_uuids_and_every_link_follows`).
Update the [diagram](architecture.md#database) and `frontend/src/lib/types.ts`.

**API.** Handler in the app, registered in `light()` or `heavy()` (moves file contents) in `web/mod.rs`; type in `types.ts`; a
test in `web/tests/`; a line in `ROUTES` (`routes.rs`). Errors are `ApiError` (`bad`, `not_found`, `conflict`) →
`{ "error": "..." }`. Every handler takes `Extension<Access>` and limits itself to the role: `check_see`/`check_write(folder)`
(others' devices answer 404), `viewer()`/`scope_args()` for file queries, `require_admin()`, `require_workspace()`. Parse ids
with `Id` and check visibility like any input. Test each role.

**Device contract.** Edit `proto/fafnirx.proto` (types regenerate). Keep it additive; never add a call that reads, lists or
deletes stored data.

**A source.** Model: `integrations/gdrive.rs`. A `sources` row plus a `source-<name>` device (`integrations/api.rs`), claimed
by `integrations::claim_source`, dispatched by `kind`; plan by comparing the remote listing with `source_items` and live
`files`; download to temp files; record with `ingest::commit_version` (`renamed_from` for renames) and `mark_deleted`; delete
nothing if listing fails.

## The web app

Vite, React 19, TypeScript, Tailwind v4, shadcn/ui, TanStack Query, React Router.

- `src/lib/api.ts` is the only JSON `fetch`; ids are strings. `src/lib/live.tsx` invalidates query families by SSE topic.
  `src/lib/upload.ts` is the upload queue. Pages load lazily; the file list is virtualized.
- `/watch/:id` (`pages/watch.tsx`) is the standalone viewer; the Files sheet reuses `InfoPanel` (`embedded`) and `WorkNote`.
- **Translations:** write every visible string in English in `t('…')` (`useT()`; `msg('…')` for strings in data shown with `t`
  later; `tr('…')` outside components); `{name}` placeholders come from the second argument. `npm run i18n` updates
  `translations/en.json` and lists gaps; `-- --check` fails if stale or incomplete. A language is `translations/<code>.json` plus
  a line in `LANGUAGES` (`src/lib/i18n.ts`); Arabic is RTL, so use `start`/`end`, not `left`/`right`.
- **Player:** `frontend/src/player` imports nothing from the app (joined in `components/app-video.tsx` and
  `lib/player-source.ts`); its backend is `crates/stream`. Keep both READMEs true.

## Releasing

Actions → **Release** → Run workflow on `master` with `X.Y.Z` (*dry_run* rehearses, *homebrew* updates the formula). No manual version change.

1. **Check:** version is `X.Y.Z`, newer than all releases, unused, not below `Cargo.toml`; publishing secrets exist; Docker Hub login works.
2. **Version commit** by `prepare-release.sh` (`Cargo.toml`, `Cargo.lock`) on branch `release/X.Y.Z`; every later job tests that commit.
3. **Verify:** CI; client builds for Windows, macOS, Linux (installer, tarballs, `.deb`, and the plain `fafnirx-X.Y.Z-<os>-<arch>` + `.sha256` clients update from); Docker image started on amd64 and arm64.
4. **Publish, only if all passed:** fast-forward `master`, push the image (`<version>`, `<major>.<minor>`, `latest`), tag and GitHub Release, then Homebrew. The temporary branch is deleted.

A failure before step 4 leaves `master` untouched; if `master` moved, nothing is published: run again. If publishing fails
halfway, run again with the same version. Secrets (repository, not environment): `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` (access
token), `HOMEBREW_TAP_TOKEN`; optional variable `DOCKERHUB_IMAGE`. The bot must be able to push to a protected `master`; add
reviewers to the `release` environment for a manual gate. `.github/scripts/smoke-image.sh <image>` runs the image check.

## Troubleshooting

`set FAFNIRX_TEST_DATABASE_URL`: start the test PostgreSQL and export it. `Cannot start a runtime from within a runtime`: a
pool was made or dropped in async code. Port 8080 busy: set `FAFNIRX_HTTP`.
