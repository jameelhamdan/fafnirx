"""Numbers for the dashboard."""

from sqlalchemy import BigInteger, func, select
from sqlalchemy.orm import Session

from . import db, fmt
from .models import Alert, Device, Event, File, Mirror, Source, Version

DAYS = 14
BAR_W, BAR_GAP, CHART_H = 26, 8, 110
# Stack order, bottom to top: (series key, actions it counts, css class)
SERIES = (
    ("added", ("add", "rename"), "s-add"),
    ("updated", ("update",), "s-update"),
    ("deleted", ("delete",), "s-delete"),
)


def totals(s: Session) -> dict:
    """What the orchestrator holds: live files (devices and sources) and files waiting for retention."""
    live_files, live_bytes = s.execute(
        select(func.count(File.id), func.coalesce(func.sum(Version.size), 0).cast(BigInteger))
        .join(Version, Version.id == File.current_version_id)
        .where(File.deleted_at.is_(None))
    ).one()
    marked = s.scalar(select(func.count(File.id)).where(File.deleted_at.is_not(None)))
    return {"files": live_files, "bytes": live_bytes, "marked_deleted": marked}


def activity(s: Session) -> dict:
    """Changes per day for the last two weeks, stacked by kind, with the geometry of the bars."""
    today = db.now() // 86400
    first = today - (DAYS - 1)
    day = Event.ts // 86400
    rows = s.execute(
        select(day.label("day"), Event.action, func.count().label("n"))
        .where(Event.ts >= first * 86400)
        .group_by(day, Event.action)
    )
    per_day = {d: {key: 0 for key, _, _ in SERIES} for d in range(first, today + 1)}
    for d, action, n in rows:
        for key, actions, _ in SERIES:
            if action in actions and d in per_day:
                per_day[d][key] += n
    peak = max([sum(v.values()) for v in per_day.values()] + [1])
    bars = []
    for i, (d, counts) in enumerate(per_day.items()):
        x, y, segments = i * (BAR_W + BAR_GAP), CHART_H, []
        for key, _, css in SERIES:
            h = round(CHART_H * counts[key] / peak, 1)
            if h:
                y -= h
                segments.append({"y": y, "h": h, "css": css})
        label = fmt.day(d * 86400)
        total = sum(counts.values())
        bars.append(
            {
                "x": x,
                "label": label,
                "short": label[5:],
                "total": total,
                "counts": counts,
                "segments": segments,
                "tip": f"{label}: " + (", ".join(f"{n} {k}" for k, n in counts.items() if n) or "no changes"),
            }
        )
    width = DAYS * (BAR_W + BAR_GAP) - BAR_GAP
    return {"bars": bars, "width": width, "height": CHART_H, "total": sum(b["total"] for b in bars), "peak": peak}


def recent_events(s: Session, limit: int = 12) -> list[dict]:
    rows = s.execute(
        select(Event, Device.name, Device.kind)
        .join(Device, Device.id == Event.device_id)
        .order_by(Event.id.desc())
        .limit(limit)
    )
    return [
        {
            "ts": e.ts,
            "action": e.action,
            "path": e.path,
            "detail": e.detail,
            "device": name.removeprefix("source:"),
            "is_source": kind == "source",
        }
        for e, name, kind in rows
    ]


def job_health(s: Session) -> dict:
    """Latest state of every source and mirror."""

    def rows(model):
        return [
            {
                "name": r.name,
                "dot": fmt.status_class(r.last_status),
                "status": r.last_status or "never run",
                "last_ok_at": r.last_ok_at,
                "last_run_at": r.last_run_at,
                "enabled": bool(r.enabled),
            }
            for r in s.scalars(select(model).order_by(model.name))
        ]

    return {"sources": rows(Source), "mirrors": rows(Mirror)}


def dashboard(s: Session) -> dict:
    devices = db.devices(s)
    return {
        "devices": devices,
        "online": sum(1 for d in devices if d["online"]),
        "totals": totals(s),
        "activity": activity(s),
        "recent": recent_events(s),
        "health": job_health(s),
        "open_alerts": s.scalar(select(func.count(Alert.id)).where(Alert.resolved_at.is_(None))),
        "disk": db.disk_usage(),
    }
