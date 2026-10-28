# Contributing

fafnirx is a small, opinionated project for homelabs. For anything larger than a bug fix, open an issue first.

## Set up

Full instructions: [docs/development.md](docs/development.md). In short:

```sh
docker run -d --name fx-test-pg -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
export FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
(cd frontend && npm ci)
cargo test --workspace
```

## Before sending a change

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd frontend && npx tsc -b && npx oxlint && npm run i18n -- --check
```

- Match the surrounding code: sparse comments that say *why*, plain names.
- A behaviour change comes with a test; remove what it makes unused; update the docs it touches.
- The device API reads nothing but client updates and shared folders, and stays additive. A new call that reads, lists or
  deletes stored data changes the security model: say so in the pull request and in the test in `crates/common`.
- Log what happened exactly once and accurately: a rename is a `rename`, never a `delete` ([what a file operation means](docs/architecture.md#what-a-file-operation-means)).

By contributing you agree that your contribution is licensed under the [MIT license](LICENSE).
