"""Formatting helpers shared by templates and JSON endpoints."""

from datetime import UTC, datetime

from . import db


def human_bytes(n) -> str:
    n = float(n or 0)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if n < 1024 or unit == "TB":
            return f"{n:.0f} {unit}" if unit == "B" else f"{n:.1f} {unit}"
        n /= 1024


def ago(ts) -> str:
    if not ts:
        return "Never"
    s = max(0, db.now() - int(ts))
    if s < 60:
        return f"{s}s ago"
    if s < 3600:
        return f"{s // 60}m ago"
    if s < 86400:
        return f"{s // 3600}h ago"
    return f"{s // 86400}d ago"


def stamp(ts) -> str:
    return datetime.fromtimestamp(int(ts), UTC).strftime("%Y-%m-%d %H:%M:%S") if ts else ""


def day(ts) -> str:
    return datetime.fromtimestamp(int(ts), UTC).strftime("%Y-%m-%d") if ts else ""


def count(n: int, noun: str) -> str:
    return f"{n} {noun}{'' if n == 1 else 's'}"


def status_class(status: str | None) -> str:
    """Class of the .dot: green, amber, grey; no class is red."""
    return {"ok": "on", "running": "on", "partial": "warn", "failed": ""}.get(status or "", "off")
