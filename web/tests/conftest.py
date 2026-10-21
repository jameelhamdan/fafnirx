import os
import uuid
from urllib.parse import quote

import psycopg
import pytest
from psycopg.rows import dict_row


@pytest.fixture()
def database_url():
    """A fresh, empty schema on the PostgreSQL server named by FAFNIRX_TEST_DATABASE_URL."""
    base = os.environ.get("FAFNIRX_TEST_DATABASE_URL")
    if not base:
        pytest.fail("set FAFNIRX_TEST_DATABASE_URL (e.g. postgresql://postgres:test@localhost:5432/postgres)")
    schema = f"t_{uuid.uuid4().hex[:12]}"
    with psycopg.connect(base, autocommit=True) as admin:
        admin.execute(f"CREATE SCHEMA {schema}")
        sep = "&" if "?" in base else "?"
        yield f"{base}{sep}options={quote(f'-c search_path={schema}')}"
        admin.execute(f"DROP SCHEMA {schema} CASCADE")


@pytest.fixture()
def dbc(database_url):
    """A connection to the test database for arranging and checking rows (autocommit, dict rows)."""
    with psycopg.connect(database_url, autocommit=True, row_factory=dict_row) as conn:
        yield conn
