"""A throwaway PostgreSQL schema for the end-to-end scripts.

Needs a server named by FAFNIRX_TEST_DATABASE_URL, for example
    docker run -d -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
    FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
Each run gets its own schema, selected through the `options` parameter of the connection URL, so the
engine, the web UI and the script's own connection all see the same empty database.
"""

import os
import uuid
from urllib.parse import quote

import psycopg


def create() -> tuple[str, callable]:
    """Returns (url for FAFNIRX_DATABASE_URL, function that drops the schema again)."""
    base = os.environ.get("FAFNIRX_TEST_DATABASE_URL")
    if not base:
        raise SystemExit("set FAFNIRX_TEST_DATABASE_URL (postgresql://user:password@host:port/database)")
    schema = f"e2e_{uuid.uuid4().hex[:12]}"
    admin = psycopg.connect(base, autocommit=True)
    admin.execute(f"CREATE SCHEMA {schema}")
    url = f"{base}{'&' if '?' in base else '?'}options={quote(f'-c search_path={schema}')}"

    def drop():
        admin.execute(f"DROP SCHEMA {schema} CASCADE")
        admin.close()

    return url, drop


def connect(url: str) -> psycopg.Connection:
    """A connection for arranging and checking rows (autocommit, tuple rows)."""
    return psycopg.connect(url, autocommit=True)
