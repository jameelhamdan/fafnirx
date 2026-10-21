"""PostgreSQL access shared with the Rust engine (same database), through SQLAlchemy."""

import hashlib
import os
import secrets
import shutil
import threading
import time
from pathlib import Path

from sqlalchemy import BigInteger, Engine, and_, create_engine, func, select
from sqlalchemy.engine import make_url
from sqlalchemy.exc import OperationalError
from sqlalchemy.orm import Session

from .models import Device, File, Setting, Version, as_dict

DEFAULT_SCHEMA = Path(__file__).resolve().parents[2] / "schema.sql"
# Serializes schema creation between the engine and the web UI, which both apply it on start.
SCHEMA_LOCK = 0x6661666E69727801
CONNECT_ATTEMPTS = 30

_engines: dict[str, Engine] = {}
_engines_lock = threading.Lock()


def data_dir() -> Path:
    return Path(os.environ.get("FAFNIRX_DATA", "data")).resolve()


def schema_path() -> Path:
    return Path(os.environ.get("FAFNIRX_SCHEMA", DEFAULT_SCHEMA))


def database_url() -> str:
    url = os.environ.get("FAFNIRX_DATABASE_URL", "")
    if not url:
        raise RuntimeError("FAFNIRX_DATABASE_URL must be set (postgresql://user:password@host/database)")
    return url


def engine() -> Engine:
    """The engine for FAFNIRX_DATABASE_URL (the same connection string the Rust engine uses)."""
    url = database_url()
    with _engines_lock:
        if url not in _engines:
            _engines[url] = create_engine(
                make_url(url).set(drivername="postgresql+psycopg"), pool_size=5, max_overflow=5, pool_pre_ping=True
            )
        return _engines[url]


def dispose_engines() -> None:
    """Closes every pooled connection (used by tests, which switch databases)."""
    with _engines_lock:
        for e in _engines.values():
            e.dispose()
        _engines.clear()


def session() -> Session:
    """A new session. Callers commit their own writes."""
    return Session(engine(), expire_on_commit=False)


def init() -> None:
    """Applies the schema, waiting for the database to come up."""
    for attempt in range(1, CONNECT_ATTEMPTS + 1):
        try:
            conn = engine().connect()
            break
        except OperationalError:
            if attempt == CONNECT_ATTEMPTS:
                raise
            time.sleep(2)
    with conn, conn.begin():
        conn.exec_driver_sql(f"SELECT pg_advisory_xact_lock({SCHEMA_LOCK})")
        conn.exec_driver_sql(schema_path().read_text(encoding="utf-8"))


def now() -> int:
    return int(time.time())


def hash_token(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_token() -> str:
    return secrets.token_urlsafe(32)


def setting(s: Session, key: str, default: int) -> int:
    row = s.get(Setting, key)
    try:
        return int(row.value) if row else default
    except ValueError:
        return default


def set_setting(s: Session, key: str, value: int) -> None:
    row = s.get(Setting, key)
    if row:
        row.value = str(value)
    else:
        s.add(Setting(key=key, value=str(value)))


def devices(s: Session, name: str | None = None) -> list[dict]:
    """Devices (all, or just `name`) with their collected totals and online state."""
    offline_after = setting(s, "offline_after_secs", 60)
    stmt = (
        select(
            Device,
            func.coalesce(func.sum(Version.size), 0).cast(BigInteger).label("collected_bytes"),
            func.count(Version.id).label("file_count"),
        )
        .outerjoin(File, and_(File.device_id == Device.id, File.deleted_at.is_(None)))
        .outerjoin(Version, Version.id == File.current_version_id)
        .where(Device.kind == "device")
        .group_by(Device.id)
        .order_by(Device.name)
    )
    if name is not None:
        stmt = stmt.where(Device.name == name)
    t = now()
    out = []
    for device, collected, count in s.execute(stmt):
        d = as_dict(device)
        d["collected_bytes"], d["file_count"] = collected, count
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
