# Development

## Prerequisites

- Rust (stable) and a C toolchain. On Windows, MSVC build tools.
- Python 3.12 or newer.
- Docker, for a PostgreSQL to test against.
- `rclone` on `PATH` (or `FAFNIRX_RCLONE`) for the mirror end-to-end test.
- Linux only, for the client tray: GTK and appindicator development packages. Build with `--no-default-features` to skip it.

## Get started

```sh
git clone <this repository> fafnirx && cd fafnirx

# a PostgreSQL the tests may create schemas in
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres

# Python environment for the web UI and the end-to-end scripts
cd web && python -m venv .venv && .venv/bin/pip install -r requirements-dev.txt && cd ..
# (Windows: .venv\Scripts\python)

cargo build --workspace
```

## Run the pieces by hand

Create a database, then start the engine and the web UI in two terminals:

```sh
export FAFNIRX_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
export FAFNIRX_DATA=./data

cargo run -p fafnirx-server                                      # gRPC on :7443

FAFNIRX_ADMIN_PASSWORD=pw web/.venv/bin/python -m fafnirx_web    # run from web/; UI on :8080
```

Then enroll a client with `cargo run -p fafnirx -- enroll ...` (see [Installation](installation.md#2-the-client)). Set
`FAFNIRX_TOKEN_STORE=file` to avoid the OS keychain while developing.

## Tests

| What | Command | Needs |
|---|---|---|
| Rust unit tests | `cargo test --workspace` | `FAFNIRX_TEST_DATABASE_URL` |
| Web UI tests | `cd web && .venv/bin/python -m pytest` | `FAFNIRX_TEST_DATABASE_URL` |
| End to end: devices, TLS, resume, updates | `web/.venv/bin/python tests/e2e.py` | Built binaries, test database |
| End to end: Drive (fake API) and mirrors | `web/.venv/bin/python tests/e2e_io.py` | Also `rclone` |
| End to end: delete, retention, GC | `web/.venv/bin/python tests/e2e_retention.py` | About 2 minutes |

- The database-backed tests create a schema of their own on the test server and drop it afterwards, so they can run in
  parallel and the server needs no cleanup.
- The end-to-end scripts start the real `fafnirx-server`, the real web app and real client binaries. They look for the
  binaries in `target/debug`, or in `$FAFNIRX_BIN_DIR` (useful when `CARGO_TARGET_DIR` is elsewhere).
- They share `tests/e2e.py` (server process, `run`, `check`) and `tests/pgtest.py` (test schema).

Before sending a change:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets
web/.venv/bin/python -m ruff format web tests
web/.venv/bin/python -m ruff check web tests
```

## Conventions

- Match the surrounding code: comments are sparse and explain *why*; names are plain.
- Rust: `rustfmt.toml` sets `max_width = 120`. Python: `ruff.toml`, line length 120.
- The web UI wording is short sentences in sentence case; the product name stays `fafnirx`.
- A change to behaviour comes with a test: a unit test where the logic is isolated, an end-to-end check when it spans
  processes.

## Database access in the engine

The engine uses the synchronous `postgres` crate behind an `r2d2` pool (`crates/server/src/db.rs`). Rules that follow
from that:

- **Never touch the database from async code.** `postgres` drives its own runtime and panics inside Tokio. Use
  `db::with(&db, |conn| ...)`, which runs the closure on a blocking thread with a pooled `postgres::Client`.
- **Never drop a `Db` (the pool) on an async thread.** For the same reason, `main` leaks one reference on purpose.
- Helpers take `&mut impl GenericClient`, so they work on both a `Client` and a `Transaction`.
- Parameters are `$1, $2`; integers are `i64` (every integer column is `BIGINT`).
- Use `INSERT ... RETURNING id` instead of "last insert id".
- Commits that must be consistent with garbage collection hold `db::CHUNK_GATE` shared (see
  [Architecture](architecture.md#background-work-in-the-engine)).

Unit tests get a fresh schema from `db::testing::db()`.

## Changing the schema

`schema.sql` is applied by both processes on every start under an advisory lock, so every statement must be
idempotent. There is no migration framework yet. To change the schema:

1. Edit the `CREATE TABLE` for fresh installs.
2. Append idempotent statements for existing installs, for example `ALTER TABLE files ADD COLUMN IF NOT EXISTS ...`.
3. Keep both engine and web UI working against the old and the new shape until every deployment has started once.
4. Update the [database diagram](architecture.md#database-structure).

## Changing the gRPC contract

Edit `proto/fafnirx.proto`; `crates/common/build.rs` regenerates the Rust types. Keep it additive: old clients stay in
the field for a long time because they update themselves only to signed builds. Preserve the one-way rule: do not add
any call that reads, lists or deletes stored data.

## Adding a source

Sources are modules in `crates/server/src`: `gdrive.rs` is the model. A source:

1. Gets a `sources` row (kind, JSON config) and a hidden `devices` row; the web UI creates both
   (`web/fafnirx_web/integrations.py`).
2. Is claimed by `sources::claim_source`, then dispatched by `kind` in `sources::run_source`.
3. Plans work by comparing the remote listing with `source_items` and the live `files`, then stores chunks in the
   `Store` and records versions through `ingest::commit_version`, `ingest::mark_deleted`.
4. Never deletes anything when listing the remote fails.

## Troubleshooting a dev setup

- `set FAFNIRX_TEST_DATABASE_URL ...`: start the test PostgreSQL (above) and export the variable.
- Windows, `UnknownIssuer` in end-to-end tests: antivirus HTTPS scanning is replacing the certificate on loopback.
- Windows: run Python through `cmd /c "... 2>&1"` if PowerShell turns warnings on stderr into errors.
