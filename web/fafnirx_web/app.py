"""fafnirx web UI and admin API (FastAPI)."""

import contextlib
import hmac
import os
import re
import secrets
import time
from pathlib import Path

from fastapi import Depends, FastAPI, Form, HTTPException, Request
from fastapi.responses import FileResponse, HTMLResponse, JSONResponse, RedirectResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from sqlalchemy import select, update
from sqlalchemy.exc import IntegrityError
from starlette.middleware.sessions import SessionMiddleware

from . import browse, db, fmt, integrations, stats
from .models import Alert, Device, Event, as_dict

BASE = Path(__file__).resolve().parent
NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]{0,62}$")
NAME_RULE = "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'"
LOGIN_WINDOW, LOGIN_MAX = 60, 5

templates = Jinja2Templates(directory=BASE / "templates")
templates.env.filters.update(bytes=fmt.human_bytes, ago=fmt.ago, stamp=fmt.stamp, day=fmt.day, count=fmt.count)


def flash(request: Request, text: str, kind: str = "ok") -> None:
    """A message shown once, on the next page."""
    request.session["flashes"] = [*request.session.get("flashes", []), {"kind": kind, "text": text}]


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
    db.database_url()
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

    def session_dep():
        with db.session() as s:
            yield s

    def require_admin(request: Request):
        if not request.session.get("admin"):
            raise HTTPException(status_code=303, headers={"Location": "/login"})

    def render(request: Request, name: str, **ctx):
        flashes = request.session.pop("flashes", [])
        return templates.TemplateResponse(
            request, name, {"admin": bool(request.session.get("admin")), "flashes": flashes, "nav": "", **ctx}
        )

    admin = [Depends(require_admin)]

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

    # --- dashboard --------------------------------------------------------

    @app.get("/", response_class=HTMLResponse, dependencies=admin)
    def dashboard(request: Request, s=Depends(session_dep)):
        alerts = [
            as_dict(a) for a in s.scalars(select(Alert).where(Alert.resolved_at.is_(None)).order_by(Alert.id.desc()))
        ]
        return render(request, "dashboard.html", nav="dashboard", alerts=alerts, **stats.dashboard(s))

    @app.post("/alerts/{alert_id}/dismiss", dependencies=admin)
    def dismiss(request: Request, alert_id: int, s=Depends(session_dep)):
        s.execute(update(Alert).where(Alert.id == alert_id, Alert.resolved_at.is_(None)).values(resolved_at=db.now()))
        s.commit()
        flash(request, "Alert dismissed")
        return RedirectResponse("/", status_code=303)

    # --- devices ----------------------------------------------------------

    def devices_page(request: Request, s, error=None, name="", status_code=200):
        resp = render(request, "devices.html", nav="devices", devices=db.devices(s), error=error, form={"name": name})
        resp.status_code = status_code
        return resp

    @app.get("/devices", response_class=HTMLResponse, dependencies=admin)
    def devices(request: Request, s=Depends(session_dep)):
        return devices_page(request, s)

    @app.get("/api/devices", dependencies=admin)
    def api_devices(s=Depends(session_dep)):
        return JSONResponse(
            [
                {
                    "name": d["name"],
                    "online": d["online"],
                    "revoked": d["revoked"],
                    "last_seen": fmt.ago(d["last_seen"]),
                    "last_collected": fmt.ago(d["last_collected"]),
                    "collected": fmt.human_bytes(d["collected_bytes"]),
                    "files": d["file_count"],
                }
                for d in db.devices(s)
            ]
        )

    @app.post("/devices", dependencies=admin)
    def create_device(request: Request, name: str = Form(...), s=Depends(session_dep)):
        name = name.strip().lower()
        if not NAME_RE.match(name):
            return devices_page(request, s, status_code=400, error=NAME_RULE, name=name)
        token = db.new_token()
        s.add(Device(name=name, token_hash=db.hash_token(token), created_at=db.now()))
        try:
            s.commit()
        except IntegrityError:
            s.rollback()
            return devices_page(request, s, status_code=400, error=f"A device named {name} already exists", name=name)
        return device_page(request, s, name, new_token=token)

    def device_page(request: Request, s, name: str, new_token: str | None = None):
        dev = next((d for d in db.devices(s) if d["name"] == name), None)
        if dev is None:
            raise HTTPException(404, "No such device")
        events = [
            as_dict(e)
            for e in s.scalars(select(Event).where(Event.device_id == dev["id"]).order_by(Event.id.desc()).limit(50))
        ]
        ca = db.data_dir() / "certs" / "ca.pem"
        server = os.environ.get("FAFNIRX_PUBLIC_ADDR") or f"https://{request.url.hostname}:7443"
        return render(
            request,
            "device.html",
            nav="devices",
            dev=dev,
            events=events,
            new_token=new_token,
            server=server,
            has_ca=ca.is_file(),
        )

    @app.get("/devices/{name}", response_class=HTMLResponse, dependencies=admin)
    def device(request: Request, name: str, s=Depends(session_dep)):
        return device_page(request, s, name)

    @app.post("/devices/{name}/token", dependencies=admin)
    def rotate_token(request: Request, name: str, s=Depends(session_dep)):
        token = db.new_token()
        n = s.execute(
            update(Device)
            .where(Device.name == name, Device.kind == "device")
            .values(token_hash=db.hash_token(token), revoked_at=None)
        ).rowcount
        s.commit()
        if not n:
            raise HTTPException(404, "No such device")
        return device_page(request, s, name, new_token=token)

    @app.post("/devices/{name}/revoke", dependencies=admin)
    def revoke(request: Request, name: str, s=Depends(session_dep)):
        s.execute(
            update(Device)
            .where(Device.name == name, Device.kind == "device", Device.revoked_at.is_(None))
            .values(revoked_at=db.now())
        )
        s.commit()
        flash(request, f"Device {name} revoked. Its data is kept.")
        return RedirectResponse(f"/devices/{name}", status_code=303)

    # --- settings ---------------------------------------------------------

    @app.get("/settings", response_class=HTMLResponse, dependencies=admin)
    def settings_page(request: Request, s=Depends(session_dep)):
        return render(
            request,
            "settings.html",
            nav="settings",
            version_count=db.setting(s, "version_count", 5),
            retention_days=db.setting(s, "retention_days", 30),
        )

    @app.post("/settings", dependencies=admin)
    def settings_save(
        request: Request, version_count: int = Form(...), retention_days: int = Form(...), s=Depends(session_dep)
    ):
        if not (1 <= version_count <= 1000 and 1 <= retention_days <= 3650):
            raise HTTPException(400, "Values out of range")
        db.set_setting(s, "version_count", version_count)
        db.set_setting(s, "retention_days", retention_days)
        s.commit()
        flash(request, "Settings saved")
        return RedirectResponse("/settings", status_code=303)

    @app.get("/ca.pem", dependencies=admin)
    def ca_pem():
        path = db.data_dir() / "certs" / "ca.pem"
        if not path.is_file():
            raise HTTPException(404, "The engine has not generated the CA yet")
        return FileResponse(path, media_type="application/x-pem-file", filename="fafnirx-ca.pem")

    integrations.register(app, session_dep, require_admin, render, flash)
    browse.register(app, session_dep, require_admin, render)
    return app
