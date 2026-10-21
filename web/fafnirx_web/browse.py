"""Read-only browser for the files the orchestrator holds, per device and per source.

It shows names, sizes, times and version history. It never offers a download: restoring is a manual copy.
"""

from fastapi import Depends, HTTPException, Request
from fastapi.responses import HTMLResponse
from sqlalchemy import BigInteger, func, select
from sqlalchemy.orm import Session

from . import db
from .models import Device, File, Source, Version

DAY = 86400


def clean_prefix(path: str) -> str:
    """'a/b' -> 'a/b/'; the root is ''."""
    path = path.strip("/")
    return path + "/" if path else ""


def crumbs(prefix: str, base: str, deleted: bool) -> list[dict]:
    flag = "&deleted=1" if deleted else ""
    out, acc = [{"name": "Files", "url": f"{base}/files" + ("?deleted=1" if deleted else "")}], ""
    for part in prefix.strip("/").split("/") if prefix else []:
        acc += part + "/"
        out.append({"name": part, "url": f"{base}/files?path={acc}{flag}"})
    return out


def list_dir(s: Session, device_id: int, prefix: str, include_deleted: bool, retention_days: int) -> tuple[list, list]:
    """(folders, files) directly under `prefix`."""
    n = len(prefix)
    rel = func.substr(File.path, n + 1)
    under = (File.device_id == device_id) & File.path.startswith(prefix, autoescape=True)
    if not include_deleted:
        under &= File.deleted_at.is_(None)

    name = func.split_part(rel, "/", 1)
    folders = [
        {
            "name": row.name,
            "path": prefix + row.name,
            "files": row.files,
            "size": row.size,
            "changed": row.changed,
            "all_deleted": row.all_deleted,
        }
        for row in s.execute(
            select(
                name.label("name"),
                func.count(File.id).label("files"),
                func.coalesce(func.sum(Version.size), 0).cast(BigInteger).label("size"),
                func.max(Version.created_at).label("changed"),
                func.bool_and(File.deleted_at.is_not(None)).label("all_deleted"),
            )
            .select_from(File)
            .outerjoin(Version, Version.id == File.current_version_id)
            .where(under, func.strpos(rel, "/") > 0)
            .group_by(name)
            .order_by(name)
        )
    ]

    version_count = select(func.count(Version.id)).where(Version.file_id == File.id).correlate(File).scalar_subquery()
    files = []
    for f, v, versions in s.execute(
        select(File, Version, version_count)
        .outerjoin(Version, Version.id == File.current_version_id)
        .where(under, func.strpos(rel, "/") == 0)
        .order_by(File.path)
    ):
        files.append(
            {
                "name": f.path[n:],
                "path": f.path,
                "size": v.size if v else None,
                "mtime": v.mtime // 1_000_000_000 if v else None,
                "changed": v.created_at if v else None,
                "versions": versions,
                "deleted_at": f.deleted_at,
                "purge_at": f.deleted_at + retention_days * DAY if f.deleted_at else None,
            }
        )
    return folders, files


def register(app, session_dep, require_admin, render):
    admin = [Depends(require_admin)]

    def owner(s: Session, kind: str, name: str) -> tuple[Device, str, str]:
        """(device row that owns the files, url base, display name) of a device or a source."""
        if kind == "devices":
            dev = s.scalar(select(Device).where(Device.name == name, Device.kind == "device"))
        else:
            src = s.scalar(select(Source).where(Source.name == name))
            dev = s.get(Device, src.device_id) if src else None
        if dev is None:
            raise HTTPException(404, f"No such {kind.removesuffix('s')}")
        return dev, f"/{kind}/{name}", name

    def files_page(request: Request, s: Session, kind: str, name: str, path: str, deleted: int):
        dev, base, shown = owner(s, kind, name)
        prefix = clean_prefix(path)
        retention = db.setting(s, "retention_days", 30)
        folders, files = list_dir(s, dev.id, prefix, bool(deleted), retention)
        return render(
            request,
            "files.html",
            nav=kind,
            kind=kind,
            owner=shown,
            base=base,
            prefix=prefix,
            crumbs=crumbs(prefix, base, bool(deleted)),
            folders=folders,
            files=files,
            deleted=bool(deleted),
            retention=retention,
        )

    def versions_page(request: Request, s: Session, kind: str, name: str, path: str):
        dev, base, shown = owner(s, kind, name)
        f = s.scalar(select(File).where(File.device_id == dev.id, File.path == path))
        if f is None:
            raise HTTPException(404, "No such file")
        versions = [
            {
                "id": v.id,
                "created_at": v.created_at,
                "size": v.size,
                "mtime": v.mtime // 1_000_000_000,
                "chunks": len(v.manifest) // 32,
                "hash": bytes(v.root_hash).hex(),
                "current": v.id == f.current_version_id,
                "pruned_at": v.pruned_at,
            }
            for v in s.scalars(select(Version).where(Version.file_id == f.id).order_by(Version.id.desc()))
        ]
        parent = path.rpartition("/")[0]
        return render(
            request,
            "versions.html",
            nav=kind,
            owner=shown,
            base=base,
            path=path,
            parent=parent,
            file=f,
            versions=versions,
            retention=db.setting(s, "retention_days", 30),
        )

    def add_routes(kind: str):
        @app.get(f"/{kind}/{{name}}/files", response_class=HTMLResponse, dependencies=admin, name=f"{kind}_files")
        def files(request: Request, name: str, path: str = "", deleted: int = 0, s=Depends(session_dep)):
            return files_page(request, s, kind, name, path, deleted)

        @app.get(
            f"/{kind}/{{name}}/files/versions", response_class=HTMLResponse, dependencies=admin, name=f"{kind}_versions"
        )
        def versions(request: Request, name: str, path: str, s=Depends(session_dep)):
            return versions_page(request, s, kind, name, path)

    add_routes("devices")
    add_routes("sources")
