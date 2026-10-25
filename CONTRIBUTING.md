# Contributing

Thanks for helping. fafnirx is a small, opinionated project for homelabs, so for anything larger than a bug fix please
open an issue first to agree on the direction.

## Getting set up

[docs/development.md](docs/development.md) explains how to build, run and test everything (Rust, Node 22, a throwaway
PostgreSQL in Docker). The short version:

```sh
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
(cd frontend && npm ci)
cargo test --workspace
```

## Before you send a change

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd frontend && npx tsc -b && npx oxlint
```

- Match the surrounding code: sparse comments that explain *why*, plain names.
- A change in behaviour comes with a test.
- Remove what your change makes unused.
- Keep the one-way rule: the device API must never gain a call that reads, lists or deletes stored data.
- Keep the device API additive; old clients stay in the field for a long time.
- Update the docs that describe what you changed.

By contributing you agree that your contribution is licensed under the [MIT license](LICENSE).
