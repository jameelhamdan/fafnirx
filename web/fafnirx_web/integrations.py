"""Third-party sources (inbound) and mirrors (outbound): configuration pages.

The engine runs them. This module only stores their configuration and requests runs
(`run_requested`). Credentials are write-only: they are never rendered back into a page.
"""

import json
import re

from fastapi import Depends, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, JSONResponse, RedirectResponse
from sqlalchemy import BigInteger, delete, func, select, update
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from . import db, fmt
from .fmt import status_class
from .models import Alert, Device, File, Mirror, MirrorItem, Source, SourceItem, Version

NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,62}$")
NAME_RULE = "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'"
FOLDER_ID_RE = re.compile(r"^[A-Za-z0-9_-]{10,100}$")
OPTION_KEY_RE = re.compile(r"^[a-z0-9_]{1,64}$")
TYPE_RE = re.compile(r"^[a-z0-9]{1,32}$")
SOURCE_PREFIX = "source:"
INTERVAL_HOURS = (0.25, 24 * 365)
MODELS = {"sources": Source, "mirrors": Mirror}


class FormError(ValueError):
    """A message that is safe to show next to the form."""


def parse_folder_ids(text: str) -> list[str]:
    """Drive folder ids, one per line. Pasted folder URLs are reduced to their id."""
    ids: list[str] = []
    for token in re.split(r"[\s,]+", text.strip()):
        if not token:
            continue
        m = re.search(r"/folders/([A-Za-z0-9_-]+)", token) or re.search(r"[?&]id=([A-Za-z0-9_-]+)", token)
        folder = m.group(1) if m else token
        if not FOLDER_ID_RE.match(folder):
            raise FormError(f"{token!r} is not a Drive folder id or folder URL")
        if folder not in ids:
            ids.append(folder)
    if not ids:
        raise FormError("Add at least one folder id")
    return ids


def parse_service_account(text: str) -> dict:
    try:
        sa = json.loads(text)
    except ValueError:
        raise FormError("The service account key is not valid JSON") from None
    if not isinstance(sa, dict) or not sa.get("client_email") or "BEGIN" not in str(sa.get("private_key", "")):
        raise FormError(
            "The service account key needs client_email and private_key (download the JSON key from Google Cloud)"
        )
    return {k: sa[k] for k in ("client_email", "private_key", "token_uri") if k in sa}


def parse_options(text: str) -> dict[str, str]:
    """`key=value` per line; blank lines and `#` comments are ignored."""
    opts: dict[str, str] = {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, sep, value = line.partition("=")
        key, value = key.strip().lower().replace("-", "_"), value.strip()
        if not sep or not OPTION_KEY_RE.match(key):
            raise FormError(f"Option {line.split('=')[0]!r}: use key=value with lowercase letters, digits and _")
        opts[key] = value
    return opts


def hours_to_secs(hours: float) -> int:
    if not (INTERVAL_HOURS[0] <= hours <= INTERVAL_HOURS[1]):
        raise FormError("Interval: between 0.25 hours and one year")
    return int(hours * 3600)


def get_row(s: Session, table: str, name: str):
    """`table` is always one of our own table names, never user input."""
    model = MODELS[table]
    row = s.scalar(select(model).where(model.name == name))
    if row is None:
        raise HTTPException(404, f"No such {table.removesuffix('s')}")
    return row


def job_state(row) -> dict:
    """What the live status refresh shows for a source or mirror."""
    running = row.last_status == "running"
    return {
        "name": row.name,
        "dot": status_class(row.last_status),
        "running": running,
        "enabled": bool(row.enabled),
        "last": "Running" if running else fmt.ago(row.last_run_at),
        "message": row.last_message or "",
        "files": getattr(row, "last_files", None),
    }


def register(app, session_dep, require_admin, render, flash):
    admin = [Depends(require_admin)]

    def register_action(table: str, action: str, done: str):
        @app.post(f"/{table}/{{name}}/{action}", dependencies=admin, name=f"{table}_{action}")
        def handler(request: Request, name: str, s=Depends(session_dep)):
            row = get_row(s, table, name)
            if action == "run":
                row.run_requested = 1
            else:
                row.enabled = 1 - row.enabled
            s.commit()
            flash(request, done.format(name=name, state="resumed" if row.enabled else "paused"))
            return RedirectResponse(f"/{table}", status_code=303)

    for table in ("sources", "mirrors"):
        register_action(table, "run", "Run requested for {name}. It starts within a few seconds.")
        register_action(table, "toggle", "{name} {state}")

    @app.get("/api/jobs", dependencies=admin)
    def api_jobs(s=Depends(session_dep)):
        return JSONResponse(
            {
                table: [job_state(r) for r in s.scalars(select(model).order_by(model.name))]
                for table, model in MODELS.items()
            }
        )

    # --- sources ----------------------------------------------------------------------

    def source_rows(s: Session) -> list[dict]:
        rows = s.execute(
            select(
                Source,
                func.coalesce(func.sum(Version.size), 0).cast(BigInteger),
                func.count(Version.id),
            )
            .outerjoin(File, (File.device_id == Source.device_id) & File.deleted_at.is_(None))
            .outerjoin(Version, Version.id == File.current_version_id)
            .group_by(Source.id)
            .order_by(Source.name)
        )
        out = []
        for src, size, files in rows:
            cfg = json.loads(src.config)
            out.append(
                {
                    **job_state(src),
                    "interval_secs": src.interval_secs,
                    "bytes": size,
                    "files": files,
                    "folders": len(cfg.get("folders", [])),
                    "account": cfg.get("service_account", {}).get("client_email", ""),
                }
            )
        return out

    def sources_page(request: Request, s, error=None, form=None, status_code=200):
        resp = render(request, "sources.html", nav="sources", sources=source_rows(s), error=error, form=form or {})
        resp.status_code = status_code
        return resp

    @app.get("/sources", response_class=HTMLResponse, dependencies=admin)
    def sources_get(request: Request, s=Depends(session_dep)):
        return sources_page(request, s)

    @app.post("/sources", dependencies=admin)
    def sources_create(
        request: Request,
        name: str = Form(...),
        folders: str = Form(...),
        service_account: str = Form(...),
        interval_hours: float = Form(24),
        s=Depends(session_dep),
    ):
        form = {"name": name, "folders": folders, "interval_hours": interval_hours}
        try:
            name = name.strip().lower()
            if not NAME_RE.match(name):
                raise FormError(NAME_RULE)
            config = {"service_account": parse_service_account(service_account), "folders": parse_folder_ids(folders)}
            secs = hours_to_secs(interval_hours)
            if s.scalar(select(Source.id).where(Source.name == name)):
                raise FormError(f"A source named {name} already exists")
        except FormError as e:
            return sources_page(request, s, error=str(e), form=form, status_code=400)
        # The source owns a hidden devices row, so its files use the same tables as device files.
        # '!' can never equal a sha256 hex digest, so nothing can authenticate as a source.
        hidden = SOURCE_PREFIX + name
        device = s.scalar(select(Device).where(Device.name == hidden))
        if device is None:
            device = Device(name=hidden, kind="source", token_hash="!", created_at=db.now())
            s.add(device)
            s.flush()
        s.add(
            Source(
                name=name,
                kind="gdrive",
                config=json.dumps(config),
                interval_secs=secs,
                device_id=device.id,
                created_at=db.now(),
            )
        )
        try:
            s.commit()
        except IntegrityError:
            s.rollback()
            return sources_page(request, s, error=f"A source named {name} already exists", form=form, status_code=400)
        flash(request, f"Source {name} added. The first run starts within a few seconds.")
        return RedirectResponse("/sources", status_code=303)

    @app.post("/sources/{name}/delete", dependencies=admin)
    def source_delete(request: Request, name: str, s=Depends(session_dep)):
        src = get_row(s, "sources", name)
        # Collected files follow the normal delete rule: kept for the retention period, then removed.
        s.execute(
            update(File).where(File.device_id == src.device_id, File.deleted_at.is_(None)).values(deleted_at=db.now())
        )
        s.execute(delete(SourceItem).where(SourceItem.source_id == src.id))
        s.execute(delete(Source).where(Source.id == src.id))
        s.execute(
            update(Alert)
            .where(Alert.kind == "source_failed", Alert.key == name, Alert.resolved_at.is_(None))
            .values(resolved_at=db.now())
        )
        s.commit()
        flash(request, f"Source {name} removed. Its files are kept for the retention period.")
        return RedirectResponse("/sources", status_code=303)

    # --- mirrors ----------------------------------------------------------------------

    def mirror_rows(s: Session) -> list[dict]:
        return [
            {
                **job_state(m),
                "remote_type": m.remote_type,
                "remote_path": m.remote_path,
                "interval_secs": m.interval_secs,
                "keep_runs": m.keep_runs,
                "option_keys": sorted(json.loads(m.options)),
            }
            for m in s.scalars(select(Mirror).order_by(Mirror.name))
        ]

    def mirrors_page(request: Request, s, error=None, form=None, status_code=200):
        resp = render(request, "mirrors.html", nav="mirrors", mirrors=mirror_rows(s), error=error, form=form or {})
        resp.status_code = status_code
        return resp

    @app.get("/mirrors", response_class=HTMLResponse, dependencies=admin)
    def mirrors_get(request: Request, s=Depends(session_dep)):
        return mirrors_page(request, s)

    @app.post("/mirrors", dependencies=admin)
    def mirrors_create(
        request: Request,
        name: str = Form(...),
        remote_type: str = Form(...),
        options: str = Form(""),
        remote_path: str = Form(""),
        interval_hours: float = Form(72),
        keep_runs: int = Form(3),
        s=Depends(session_dep),
    ):
        form = {
            "name": name,
            "remote_type": remote_type,
            "remote_path": remote_path,
            "interval_hours": interval_hours,
            "keep_runs": keep_runs,
        }
        try:
            name, remote_type = name.strip().lower(), remote_type.strip().lower()
            if not NAME_RE.match(name):
                raise FormError(NAME_RULE)
            if not TYPE_RE.match(remote_type):
                raise FormError("Type: an rclone backend name such as s3, sftp, webdav, b2")
            opts = parse_options(options)
            secs = hours_to_secs(interval_hours)
            if not (1 <= keep_runs <= 100):
                raise FormError("Versions kept: between 1 and 100 runs")
            if s.scalar(select(Mirror.id).where(Mirror.name == name)):
                raise FormError(f"A mirror named {name} already exists")
        except FormError as e:
            return mirrors_page(request, s, error=str(e), form=form, status_code=400)
        s.add(
            Mirror(
                name=name,
                remote_type=remote_type,
                options=json.dumps(opts),
                remote_path=remote_path.strip(),
                interval_secs=secs,
                keep_runs=keep_runs,
                created_at=db.now(),
            )
        )
        try:
            s.commit()
        except IntegrityError:
            s.rollback()
            return mirrors_page(request, s, error=f"A mirror named {name} already exists", form=form, status_code=400)
        flash(request, f"Mirror {name} added. The first run starts within a few seconds.")
        return RedirectResponse("/mirrors", status_code=303)

    @app.post("/mirrors/{name}/resync", dependencies=admin)
    def mirror_resync(request: Request, name: str, s=Depends(session_dep)):
        """Forget what was uploaded: the next run re-sends everything (use after emptying the remote)."""
        m = get_row(s, "mirrors", name)
        s.execute(delete(MirrorItem).where(MirrorItem.mirror_id == m.id))
        m.run_requested = 1
        s.commit()
        flash(request, f"Mirror {name} will re-send every file on its next run.")
        return RedirectResponse("/mirrors", status_code=303)

    @app.post("/mirrors/{name}/delete", dependencies=admin)
    def mirror_delete(request: Request, name: str, s=Depends(session_dep)):
        m = get_row(s, "mirrors", name)
        s.execute(delete(MirrorItem).where(MirrorItem.mirror_id == m.id))
        s.execute(delete(Mirror).where(Mirror.id == m.id))
        s.execute(
            update(Alert)
            .where(Alert.kind == "mirror_failed", Alert.key == name, Alert.resolved_at.is_(None))
            .values(resolved_at=db.now())
        )
        s.commit()
        flash(request, f"Mirror {name} removed. Data already on the remote is not touched.")
        return RedirectResponse("/mirrors", status_code=303)
