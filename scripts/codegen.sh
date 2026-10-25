#!/bin/sh
# Regenerates crates/db (the typed queries) from queries/*.sql against the schema in crates/master/migrations.
# Needs the cornucopia CLI (`cargo install cornucopia --locked`, 1.0.x) and a PostgreSQL server it may create a
# temporary database in, given without a database name:
#
#   DATABASE_URL=postgresql://postgres:test@localhost:55432 scripts/codegen.sh
#
# CI runs it and fails when the result differs from what is committed.
set -eu
cd "$(dirname "$0")/.."
: "${DATABASE_URL:?set DATABASE_URL to a PostgreSQL server, for example postgresql://postgres:test@localhost:55432}"
# shellcheck disable=SC2046 # the migration names have no spaces
cornucopia fresh --url "$DATABASE_URL" $(ls crates/master/migrations/*.sql | sort)
