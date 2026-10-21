"""fafnirx web UI and admin API (FastAPI)."""

import contextlib
import hmac
import os
import re
import secrets
import time
from datetime import UTC, datetime
from pathlib import Path

import psycopg.errors
from fastapi import Depends, FastAPI, Form, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from starlette.middleware.sessions import SessionMiddleware

from . import db, integrations

BASE = Path(__file__).resolve().parent
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,62}$")
LOGIN_WINDOW, LOGIN_MAX = 60, 5

templates = Jinja2Templates(directory=BASE / "templates")


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


templates.env.filters.update(bytes=human_bytes, ago=ago, stamp=stamp)


def admin_password() -> str:
    pw = os.environ.get("FAFNIRX_ADMIN_PASSWORD", "")
    if not pw:
        raise RuntimeError("FAFNIRX_ADMIN_PASSWORD must be set")
    return pw


def session_secret() -> str:
    path = db.data_dir() / "web_secret"
    if not path.is_file():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(secrets.token_hex(32))
        with contextlib.suppress(OSError):  # not supported on every filesystem
            path.chmod(0o600)
    return path.read_text().strip()


def create_app() -> FastAPI:
    admin_password()  # fail fast when the password is not configured
    db.init()
    app = FastAPI(title="fafnirx", docs_url=None, redoc_url=None, openapi_url=None)
    app.add_middleware(
        SessionMiddleware,
        secret_key=session_secret(),
        same_site="strict",
        max_age=7 * 86400,
        session_cookie="fx_session",
    )
    app.mount("/static", StaticFiles(directory=BASE / "static"), name="static")
    failures: dict[str, list[float]] = {}

    def conn_dep():
        conn = db.connect()
        try:
            yield conn
        finally:
            conn.close()

    def require_admin(request: Request):
        if not request.session.get("admin"):
            raise HTTPException(status_code=303, headers={"Location": "/login"})

    def render(request: Request, name: str, **ctx):
        return templates.TemplateResponse(request, name, {"admin": bool(request.session.get("admin")), **ctx})

    # --- auth -------------------------------------------------------------

    @app.get("/login", response_class=HTMLResponse)
    def login_form(request: Request):
        return render(request, "login.html", error=None)

    @app.post("/login")
    def login(request: Request, password: str = Form(...)):
        ip = request.client.host if request.client else "?"
        now = time.time()
        recent = [t for t in failures.get(ip, []) if now - t <= LOGIN_WINDOW]
        if len(recent) >= LOGIN_MAX:
            failures[ip] = recent
            return render(request, "login.html", error="Too many attempts, wait a minute")
        if not hmac.compare_digest(password.encode(), admin_password().encode()):
            failures[ip] = [*recent, now]
            return render(request, "login.html", error="Wrong password")
        failures.pop(ip, None)
        request.session["admin"] = True
        return RedirectResponse("/", status_code=303)

    @app.get("/logout")
    def logout(request: Request):
        request.session.clear()
        return RedirectResponse("/login", status_code=303)

    # --- pages ------------------------------------------------------------

    def alerts(conn):
        return [dict(r) for r in conn.execute("SELECT * FROM alerts WHERE resolved_at IS NULL ORDER BY id DESC")]

    def index_page(request: Request, conn, error=None, status_code=200):
        resp = render(
            request, "index.html", devices=db.devices(conn), alerts=alerts(conn), disk=db.disk_usage(), error=error
        )
        resp.status_code = status_code
        return resp

    @app.get("/", response_class=HTMLResponse, dependencies=[Depends(require_admin)])
    def index(request: Request, conn=Depends(conn_dep)):
        return index_page(request, conn)

    @app.get("/api/devices", dependencies=[Depends(require_admin)])
    def api_devices(conn=Depends(conn_dep)):
        return JSONResponse(
            [
                {
                    "name": d["name"],
                    "online": d["online"],
                    "revoked": d["revoked"],
                    "last_seen": ago(d["last_seen"]),
                    "last_collected": ago(d["last_collected"]),
                    "collected": human_bytes(d["collected_bytes"]),
                    "files": d["file_count"],
                }
                for d in db.devices(conn)
            ]
        )

    @app.post("/devices", dependencies=[Depends(require_admin)])
    def create_device(request: Request, name: str = Form(...), conn=Depends(conn_dep)):
        name = name.strip().lower()
        if not NAME_RE.match(name):
            return index_page(
                request,
                conn,
                status_code=400,
                error="Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'",
            )
        token = db.new_token()
        try:
            conn.execute(
                "INSERT INTO devices (name, token_hash, created_at) VALUES (%s, %s, %s)",
                (name, db.hash_token(token), db.now()),
            )
            conn.commit()
        except psycopg.errors.UniqueViolation:
            conn.rollback()
            return index_page(request, conn, status_code=400, error=f"A device named {name} already exists")
        return device_page(request, conn, name, new_token=token)

    def device_page(request: Request, conn, name: str, new_token: str | None = None):
        dev = next((d for d in db.devices(conn) if d["name"] == name), None)
        if dev is None:
            raise HTTPException(404, "No such device")
        events = [
            dict(r)
            for r in conn.execute("SELECT * FROM events WHERE device_id = %s ORDER BY id DESC LIMIT 50", (dev["id"],))
        ]
        ca = db.data_dir() / "certs" / "ca.pem"
        server = os.environ.get("FAFNIRX_PUBLIC_ADDR") or f"https://{request.url.hostname}:7443"
        return render(
            request,
            "device.html",
            dev=dev,
            events=events,
            new_token=new_token,
            server=server,
            has_ca=ca.is_file(),
        )

    @app.get("/devices/{name}", response_class=HTMLResponse, dependencies=[Depends(require_admin)])
    def device(request: Request, name: str, conn=Depends(conn_dep)):
        return device_page(request, conn, name)

    @app.post("/devices/{name}/token", dependencies=[Depends(require_admin)])
    def rotate_token(request: Request, name: str, conn=Depends(conn_dep)):
        token = db.new_token()
        n = conn.execute(
            "UPDATE devices SET token_hash = %s, revoked_at = NULL WHERE name = %s AND kind = 'device'",
            (db.hash_token(token), name),
        ).rowcount
        conn.commit()
        if not n:
            raise HTTPException(404, "No such device")
        return device_page(request, conn, name, new_token=token)

    @app.post("/devices/{name}/revoke", dependencies=[Depends(require_admin)])
    def revoke(name: str, conn=Depends(conn_dep)):
        conn.execute(
            "UPDATE devices SET revoked_at = %s WHERE name = %s AND kind = 'device' AND revoked_at IS NULL",
            (db.now(), name),
        )
        conn.commit()
        return RedirectResponse(f"/devices/{name}", status_code=303)

    @app.post("/alerts/{alert_id}/dismiss", dependencies=[Depends(require_admin)])
    def dismiss(alert_id: int, conn=Depends(conn_dep)):
        conn.execute("UPDATE alerts SET resolved_at = %s WHERE id = %s AND resolved_at IS NULL", (db.now(), alert_id))
        conn.commit()
        return RedirectResponse("/", status_code=303)

    @app.get("/settings", response_class=HTMLResponse, dependencies=[Depends(require_admin)])
    def settings_page(request: Request, conn=Depends(conn_dep)):
        return render(
            request,
            "settings.html",
            version_count=db.setting(conn, "version_count", 5),
            retention_days=db.setting(conn, "retention_days", 30),
            saved=request.query_params.get("saved") == "1",
        )

    @app.post("/settings", dependencies=[Depends(require_admin)])
    def settings_save(version_count: int = Form(...), retention_days: int = Form(...), conn=Depends(conn_dep)):
        if not (1 <= version_count <= 1000 and 1 <= retention_days <= 3650):
            raise HTTPException(400, "Values out of range")
        db.set_setting(conn, "version_count", version_count)
        db.set_setting(conn, "retention_days", retention_days)
        conn.commit()
        return RedirectResponse("/settings?saved=1", status_code=303)

    @app.get("/ca.pem", dependencies=[Depends(require_admin)])
    def ca_pem():
        path = db.data_dir() / "certs" / "ca.pem"
        if not path.is_file():
            raise HTTPException(404, "The engine has not generated the CA yet")
        return FileResponse(path, media_type="application/x-pem-file", filename="fafnirx-ca.pem")

    integrations.register(app, conn_dep, require_admin, render)
    return app
