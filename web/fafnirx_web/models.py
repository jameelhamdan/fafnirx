"""ORM models for the tables in schema.sql.

schema.sql stays the source of truth: both the Rust engine and this app apply it on start. These classes only
describe it, and tests/test_models.py fails when they drift apart. Columns the database fills in itself
(identity ids, defaults) carry a server_default, so the ORM leaves them out of INSERTs.
"""

from sqlalchemy import BigInteger, ForeignKey, LargeBinary, Text, UniqueConstraint, text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column


class Base(DeclarativeBase):
    type_annotation_map = {int: BigInteger, str: Text, bytes: LargeBinary}


def _id() -> Mapped[int]:
    return mapped_column(primary_key=True)


def _zero(value: int = 0) -> Mapped[int]:
    return mapped_column(server_default=text(str(value)))


class Device(Base):
    """A client, or the hidden row that owns a source's files (kind = 'source')."""

    __tablename__ = "devices"

    id: Mapped[int] = _id()
    name: Mapped[str] = mapped_column(unique=True)
    kind: Mapped[str] = mapped_column(server_default=text("'device'"))
    token_hash: Mapped[str]
    created_at: Mapped[int]
    revoked_at: Mapped[int | None]
    hostname: Mapped[str | None]
    os: Mapped[str | None]
    ip: Mapped[str | None]
    client_version: Mapped[str | None]
    disk_total: Mapped[int | None]
    disk_free: Mapped[int | None]
    last_seen: Mapped[int | None]
    last_collected: Mapped[int | None]


class File(Base):
    __tablename__ = "files"
    __table_args__ = (UniqueConstraint("device_id", "path"),)

    id: Mapped[int] = _id()
    device_id: Mapped[int] = mapped_column(ForeignKey("devices.id"))
    path: Mapped[str]
    current_version_id: Mapped[int | None]
    deleted_at: Mapped[int | None]


class Version(Base):
    __tablename__ = "versions"

    id: Mapped[int] = _id()
    file_id: Mapped[int] = mapped_column(ForeignKey("files.id"))
    root_hash: Mapped[bytes]
    size: Mapped[int]
    mtime: Mapped[int]  # unix nanoseconds
    mode: Mapped[int] = _zero()
    manifest: Mapped[bytes]  # concatenated 32-byte chunk hashes
    created_at: Mapped[int]
    pruned_at: Mapped[int | None]


class Event(Base):
    __tablename__ = "events"

    id: Mapped[int] = _id()
    device_id: Mapped[int] = mapped_column(ForeignKey("devices.id"))
    path: Mapped[str]
    action: Mapped[str]  # add | update | delete | rename
    ts: Mapped[int]
    hash: Mapped[str | None]
    detail: Mapped[str | None]


class Alert(Base):
    __tablename__ = "alerts"

    id: Mapped[int] = _id()
    kind: Mapped[str]
    key: Mapped[str] = mapped_column(server_default=text("''"))
    message: Mapped[str]
    created_at: Mapped[int]
    updated_at: Mapped[int | None]
    resolved_at: Mapped[int | None]


class Source(Base):
    __tablename__ = "sources"

    id: Mapped[int] = _id()
    name: Mapped[str] = mapped_column(unique=True)
    kind: Mapped[str]
    config: Mapped[str]  # JSON, includes credentials
    interval_secs: Mapped[int] = _zero(86400)
    enabled: Mapped[int] = _zero(1)
    device_id: Mapped[int] = mapped_column(ForeignKey("devices.id"))
    run_requested: Mapped[int] = _zero()
    last_run_at: Mapped[int | None]
    last_ok_at: Mapped[int | None]
    last_status: Mapped[str | None]
    last_message: Mapped[str | None]
    created_at: Mapped[int]


class SourceItem(Base):
    __tablename__ = "source_items"

    source_id: Mapped[int] = mapped_column(ForeignKey("sources.id", ondelete="CASCADE"), primary_key=True)
    remote_id: Mapped[str] = mapped_column(primary_key=True)
    path: Mapped[str]
    remote_version: Mapped[str]


class Mirror(Base):
    __tablename__ = "mirrors"

    id: Mapped[int] = _id()
    name: Mapped[str] = mapped_column(unique=True)
    remote_type: Mapped[str]
    options: Mapped[str]  # JSON, includes credentials
    remote_path: Mapped[str]
    interval_secs: Mapped[int] = _zero(259200)
    keep_runs: Mapped[int] = _zero(3)
    enabled: Mapped[int] = _zero(1)
    run_requested: Mapped[int] = _zero()
    last_run_at: Mapped[int | None]
    last_ok_at: Mapped[int | None]
    last_status: Mapped[str | None]
    last_message: Mapped[str | None]
    last_files: Mapped[int | None]
    last_bytes: Mapped[int | None]
    created_at: Mapped[int]


class MirrorItem(Base):
    __tablename__ = "mirror_items"

    mirror_id: Mapped[int] = mapped_column(ForeignKey("mirrors.id", ondelete="CASCADE"), primary_key=True)
    device_id: Mapped[int] = mapped_column(primary_key=True)
    path: Mapped[str] = mapped_column(primary_key=True)
    root_hash: Mapped[bytes]


class Setting(Base):
    __tablename__ = "settings"

    key: Mapped[str] = mapped_column(primary_key=True)
    value: Mapped[str]


def as_dict(row: Base) -> dict:
    """The column values of a row, for templates and JSON."""
    return {c.key: getattr(row, c.key) for c in row.__table__.columns}
