"""Third-party sources (inbound) and mirrors (outbound): configuration pages.

The engine runs them. This module only stores their configuration and requests runs
(`run_requested`). Credentials are write-only: they are never rendered back into a page.
"""

import json
import re

import psycopg
from fastapi import Depends, Form, HTTPException, Request
from fastapi.responses import HTMLResponse, RedirectResponse

from . import db

NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,62}$")
FOLDER_ID_RE = re.compile(r"^[A-Za-z0-9_-]{10,100}$")
OPTION_KEY_RE = re.compile(r"^[a-z0-9_]{1,64}$")
TYPE_RE = re.compile(r"^[a-z0-9]{1,32}$")
SOURCE_PREFIX = "source:"
INTERVAL_HOURS = (0.25, 24 * 365)


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


def status_class(status: str | None) -> str:
    # css classes of .dot: green, amber, grey; no class is red
    return {"ok": "on", "running": "on", "partial": "warn", "failed": ""}.get(status or "", "off")


def get_row(conn: psycopg.Connection, table: str, name: str) -> dict:
    """`table` is always one of our own table names, never user input."""
    row = conn.execute(f"SELECT * FROM {table} WHERE name = %s", (name,)).fetchone()
    if row is None:
        raise HTTPException(404, f"No such {table.removesuffix('s')}")
    return row


def register(app, conn_dep, require_admin, render):
    admin = [Depends(require_admin)]

    def register_action(table: str, action: str, change: str):
        @app.post(f"/{table}/{{name}}/{action}", dependencies=admin)
        def handler(name: str, conn=Depends(conn_dep)):
            get_row(conn, table, name)
            conn.execute(f"UPDATE {table} SET {change} WHERE name = %s", (name,))
            conn.commit()
            return RedirectResponse(f"/{table}", status_code=303)

    for table in ("sources", "mirrors"):
        register_action(table, "run", "run_requested = 1")
        register_action(table, "toggle", "enabled = 1 - enabled")

    # --- sources ----------------------------------------------------------------------

    def source_rows(conn: psycopg.Connection) -> list[dict]:
        rows = conn.execute(
            """
            SELECT s.*, COALESCE(SUM(v.size), 0)::BIGINT AS bytes, COUNT(v.id) AS files
            FROM sources s
            LEFT JOIN files f ON f.device_id = s.device_id AND f.deleted_at IS NULL
            LEFT JOIN versions v ON v.id = f.current_version_id
            GROUP BY s.id ORDER BY s.name
            """
        ).fetchall()
        out = []
        for r in rows:
            d = dict(r)
            cfg = json.loads(d.pop("config"))
            d["folders"] = len(cfg.get("folders", []))
            d["account"] = cfg.get("service_account", {}).get("client_email", "")
            d["dot"] = status_class(d["last_status"])
            out.append(d)
        return out

    def sources_page(request: Request, conn, error=None, form=None, status_code=200):
        resp = render(request, "sources.html", sources=source_rows(conn), error=error, form=form or {})
        resp.status_code = status_code
        return resp

    @app.get("/sources", response_class=HTMLResponse, dependencies=admin)
    def sources_get(request: Request, conn=Depends(conn_dep)):
        return sources_page(request, conn)

    @app.post("/sources", dependencies=admin)
    def sources_create(
        request: Request,
        name: str = Form(...),
        folders: str = Form(...),
        service_account: str = Form(...),
        interval_hours: float = Form(24),
        conn=Depends(conn_dep),
    ):
        form = {"name": name, "folders": folders, "interval_hours": interval_hours}
        try:
            name = name.strip().lower()
            if not NAME_RE.match(name):
                raise FormError("Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'")
            config = {"service_account": parse_service_account(service_account), "folders": parse_folder_ids(folders)}
            secs = hours_to_secs(interval_hours)
            if conn.execute("SELECT 1 FROM sources WHERE name = %s", (name,)).fetchone():
                raise FormError(f"A source named {name} already exists")
        except FormError as e:
            return sources_page(request, conn, error=str(e), form=form, status_code=400)
        # The source owns a hidden devices row, so its files use the same tables as device files.
        # '!' can never equal a sha256 hex digest, so nothing can authenticate as a source.
        hidden = SOURCE_PREFIX + name
        row = conn.execute("SELECT id FROM devices WHERE name = %s", (hidden,)).fetchone()
        if row:
            device_id = row["id"]
        else:
            device_id = conn.execute(
                "INSERT INTO devices (name, kind, token_hash, created_at) VALUES (%s, 'source', '!', %s) RETURNING id",
                (hidden, db.now()),
            ).fetchone()["id"]
        conn.execute(
            "INSERT INTO sources (name, kind, config, interval_secs, device_id, created_at) VALUES (%s, 'gdrive', %s, %s, %s, %s)",
            (name, json.dumps(config), secs, device_id, db.now()),
        )
        conn.commit()
        return RedirectResponse("/sources", status_code=303)

    @app.post("/sources/{name}/delete", dependencies=admin)
    def source_delete(name: str, conn=Depends(conn_dep)):
        src = get_row(conn, "sources", name)
        # Collected files follow the normal delete rule: kept for the retention period, then removed.
        conn.execute(
            "UPDATE files SET deleted_at = %s WHERE device_id = %s AND deleted_at IS NULL", (db.now(), src["device_id"])
        )
        conn.execute("DELETE FROM source_items WHERE source_id = %s", (src["id"],))
        conn.execute("DELETE FROM sources WHERE id = %s", (src["id"],))
        conn.execute(
            "UPDATE alerts SET resolved_at = %s WHERE kind = 'source_failed' AND key = %s AND resolved_at IS NULL",
            (db.now(), name),
        )
        conn.commit()
        return RedirectResponse("/sources", status_code=303)

    # --- mirrors ----------------------------------------------------------------------

    def mirror_rows(conn: psycopg.Connection) -> list[dict]:
        out = []
        for r in conn.execute("SELECT * FROM mirrors ORDER BY name"):
            d = dict(r)
            d["option_keys"] = sorted(json.loads(d.pop("options")))
            d["dot"] = status_class(d["last_status"])
            out.append(d)
        return out

    def mirrors_page(request: Request, conn, error=None, form=None, status_code=200):
        resp = render(request, "mirrors.html", mirrors=mirror_rows(conn), error=error, form=form or {})
        resp.status_code = status_code
        return resp

    @app.get("/mirrors", response_class=HTMLResponse, dependencies=admin)
    def mirrors_get(request: Request, conn=Depends(conn_dep)):
        return mirrors_page(request, conn)

    @app.post("/mirrors", dependencies=admin)
    def mirrors_create(
        request: Request,
        name: str = Form(...),
        remote_type: str = Form(...),
        options: str = Form(""),
        remote_path: str = Form(""),
        interval_hours: float = Form(72),
        keep_runs: int = Form(3),
        conn=Depends(conn_dep),
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
                raise FormError("Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'")
            if not TYPE_RE.match(remote_type):
                raise FormError("Type: an rclone backend name such as s3, sftp, webdav, b2")
            opts = parse_options(options)
            secs = hours_to_secs(interval_hours)
            if not (1 <= keep_runs <= 100):
                raise FormError("Versions kept: between 1 and 100 runs")
            if conn.execute("SELECT 1 FROM mirrors WHERE name = %s", (name,)).fetchone():
                raise FormError(f"A mirror named {name} already exists")
        except FormError as e:
            return mirrors_page(request, conn, error=str(e), form=form, status_code=400)
        conn.execute(
            "INSERT INTO mirrors (name, remote_type, options, remote_path, interval_secs, keep_runs, created_at) VALUES (%s, %s, %s, %s, %s, %s, %s)",
            (name, remote_type, json.dumps(opts), remote_path.strip(), secs, keep_runs, db.now()),
        )
        conn.commit()
        return RedirectResponse("/mirrors", status_code=303)

    @app.post("/mirrors/{name}/resync", dependencies=admin)
    def mirror_resync(name: str, conn=Depends(conn_dep)):
        """Forget what was uploaded: the next run re-sends everything (use after emptying the remote)."""
        m = get_row(conn, "mirrors", name)
        conn.execute("DELETE FROM mirror_items WHERE mirror_id = %s", (m["id"],))
        conn.execute("UPDATE mirrors SET run_requested = 1 WHERE id = %s", (m["id"],))
        conn.commit()
        return RedirectResponse("/mirrors", status_code=303)

    @app.post("/mirrors/{name}/delete", dependencies=admin)
    def mirror_delete(name: str, conn=Depends(conn_dep)):
        m = get_row(conn, "mirrors", name)
        conn.execute("DELETE FROM mirror_items WHERE mirror_id = %s", (m["id"],))
        conn.execute("DELETE FROM mirrors WHERE id = %s", (m["id"],))
        conn.execute(
            "UPDATE alerts SET resolved_at = %s WHERE kind = 'mirror_failed' AND key = %s AND resolved_at IS NULL",
            (db.now(), name),
        )
        conn.commit()
        return RedirectResponse("/mirrors", status_code=303)
