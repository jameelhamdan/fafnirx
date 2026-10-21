"""PostgreSQL access shared with the Rust engine (same database)."""

import hashlib
import os
import secrets
import shutil
import time
from pathlib import Path

import psycopg
from psycopg.rows import dict_row

DEFAULT_SCHEMA = Path(__file__).resolve().parents[2] / "schema.sql"
# Serializes schema creation between the engine and the web UI, which both apply it on start.
SCHEMA_LOCK = 0x6661666E69727801
CONNECT_ATTEMPTS = 30


def data_dir() -> Path:
    return Path(os.environ.get("FAFNIRX_DATA", "data")).resolve()


def schema_path() -> Path:
    return Path(os.environ.get("FAFNIRX_SCHEMA", DEFAULT_SCHEMA))


def database_url() -> str:
    url = os.environ.get("FAFNIRX_DATABASE_URL", "")
    if not url:
        raise RuntimeError("FAFNIRX_DATABASE_URL must be set (postgresql://user:password@host/database)")
    return url


def connect() -> psycopg.Connection:
    """A new connection; rows are dicts. Callers commit their own writes."""
    return psycopg.connect(database_url(), row_factory=dict_row, connect_timeout=10)


def init() -> None:
    """Applies the schema, waiting for the database to come up."""
    for attempt in range(1, CONNECT_ATTEMPTS + 1):
        try:
            conn = connect()
            break
        except psycopg.OperationalError:
            if attempt == CONNECT_ATTEMPTS:
                raise
            time.sleep(2)
    with conn:
        conn.execute("SELECT pg_advisory_xact_lock(%s)", (SCHEMA_LOCK,))
        conn.execute(schema_path().read_text(encoding="utf-8"))
    conn.close()


def now() -> int:
    return int(time.time())


def hash_token(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_token() -> str:
    return secrets.token_urlsafe(32)


def setting(conn: psycopg.Connection, key: str, default: int) -> int:
    row = conn.execute("SELECT value FROM settings WHERE key = %s", (key,)).fetchone()
    try:
        return int(row["value"]) if row else default
    except ValueError:
        return default


def set_setting(conn: psycopg.Connection, key: str, value: int) -> None:
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (%s, %s) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        (key, str(value)),
    )


def devices(conn: psycopg.Connection) -> list[dict]:
    """Every device with its collected totals and online state."""
    offline_after = setting(conn, "offline_after_secs", 60)
    rows = conn.execute(
        """
        SELECT d.*,
               COALESCE(SUM(v.size), 0)::BIGINT AS collected_bytes,
               COUNT(v.id) AS file_count
        FROM devices d
        LEFT JOIN files f ON f.device_id = d.id AND f.deleted_at IS NULL
        LEFT JOIN versions v ON v.id = f.current_version_id
        WHERE d.kind = 'device'
        GROUP BY d.id
        ORDER BY d.name
        """
    ).fetchall()
    t = now()
    out = []
    for r in rows:
        d = dict(r)
        d["revoked"] = d["revoked_at"] is not None
        d["online"] = not d["revoked"] and d["last_seen"] is not None and t - d["last_seen"] <= offline_after
        out.append(d)
    return out


def disk_usage() -> dict | None:
    try:
        u = shutil.disk_usage(data_dir())
    except OSError:
        return None
    return {"total": u.total, "used": u.used, "free": u.free, "percent": round(100 * u.used / u.total)}
