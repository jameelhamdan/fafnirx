"""End-to-end test for sources (Google Drive) and mirrors (rclone).

A fake Google Drive HTTP server stands in for Google: it verifies the RS256 service-account
assertion, pages its listings, and can fail on demand. The mirror target is a local directory
through rclone's `local` backend. Everything else (engine, web UI, client) is the real thing.

    web/.venv/Scripts/python tests/e2e_io.py [bin_dir]

Needs rclone on PATH or in $FAFNIRX_RCLONE.
"""

import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import jwt
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa

sys.path.insert(0, str(Path(__file__).resolve().parent))
import e2e  # noqa: E402  (shared helpers: Server, run, check, free_port, ...)
import pgtest  # noqa: E402
from e2e import CHUNK, CLIENT, SERVER, Server, check, free_port, run  # noqa: E402

RCLONE = os.environ.get("FAFNIRX_RCLONE") or shutil.which("rclone")
FOLDER = "ROOTFOLDER0001"
SUBFOLDER = "SUBFOLDER0002"
DOC_MIME = "application/vnd.google-apps.document"
FOLDER_MIME = "application/vnd.google-apps.folder"


class FakeDrive:
    """Just enough of the Drive v3 API: token endpoint, files.get, files.list, alt=media."""

    def __init__(self, public_key):
        self.public_key = public_key
        self.items = {}  # id -> dict(name, mime, parent, data, modified)
        self.fail_listing = False
        self.flaky_once = set()  # file ids whose first download returns 503
        self.media_requests = []
        self.tokens_issued = 0
        self.port = free_port()
        drive = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def send(self, code, body, ctype="application/json"):
                raw = body if isinstance(body, bytes) else json.dumps(body).encode()
                self.send_response(code)
                self.send_header("Content-Type", ctype)
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            def do_POST(self):
                body = parse_qs(self.rfile.read(int(self.headers.get("Content-Length", 0))).decode())
                try:
                    assert body["grant_type"] == ["urn:ietf:params:oauth:grant-type:jwt-bearer"]
                    claims = jwt.decode(
                        body["assertion"][0],
                        drive.public_key,
                        algorithms=["RS256"],
                        audience=f"http://127.0.0.1:{drive.port}/token",
                    )
                    assert claims["scope"].endswith("drive.readonly") and claims["iss"].startswith("bot@")
                except Exception as e:  # signature, audience, claims
                    return self.send(400, {"error": "invalid_grant", "error_description": str(e)})
                drive.tokens_issued += 1
                self.send(200, {"access_token": "test-token", "expires_in": 3600})

            def do_GET(self):
                if self.headers.get("Authorization") != "Bearer test-token":
                    return self.send(401, {"error": "unauthorized"})
                url = urlparse(self.path)
                q = {k: v[0] for k, v in parse_qs(url.query).items()}
                if url.path == "/drive/v3/files":
                    if drive.fail_listing:
                        return self.send(403, {"error": {"message": "forbidden"}})
                    m = re.match(r"'([^']+)' in parents and trashed = false", q["q"])
                    kids = sorted((i for i, v in drive.items.items() if v["parent"] == m.group(1)))
                    start = int(q.get("pageToken", 0))
                    page = kids[start : start + 2]  # tiny pages exercise pagination
                    out = {"files": [drive.describe(i) for i in page]}
                    if start + 2 < len(kids):
                        out["nextPageToken"] = str(start + 2)
                    return self.send(200, out)
                fid = url.path.rsplit("/", 1)[-1]
                item = drive.items.get(fid)
                if item is None:
                    return self.send(404, {"error": {"message": "not found"}})
                if q.get("alt") == "media":
                    drive.media_requests.append(fid)
                    if fid in drive.flaky_once:
                        drive.flaky_once.discard(fid)
                        return self.send(503, {"error": "try later"})
                    return self.send(200, item["data"], "application/octet-stream")
                return self.send(200, {"name": item["name"]})

        self.http = ThreadingHTTPServer(("127.0.0.1", self.port), Handler)
        threading.Thread(target=self.http.serve_forever, daemon=True).start()

    def describe(self, fid):
        v = self.items[fid]
        d = {"id": fid, "name": v["name"], "mimeType": v["mime"], "modifiedTime": v["modified"]}
        if v["data"] is not None:
            d["size"] = str(len(v["data"]))
            d["md5Checksum"] = hashlib.md5(v["data"]).hexdigest()
        return d

    def put(
        self, fid, name, data=b"", parent=FOLDER, mime="application/octet-stream", modified="2024-05-01T10:00:00.000Z"
    ):
        self.items[fid] = {"name": name, "mime": mime, "parent": parent, "data": data, "modified": modified}

    def stop(self):
        self.http.shutdown()


def db_rows(conn, sql, *args):
    return conn.execute(sql, args).fetchall()


def wait_for(cond, what, timeout=60):
    deadline = time.time() + timeout
    while time.time() < deadline:
        value = cond()
        if value:
            return value
        time.sleep(0.3)
    raise SystemExit(f"FAIL: timed out waiting for {what}")


def tree(root: Path):
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob("*") if p.is_file()}


def main():
    if not RCLONE:
        raise SystemExit("rclone not found; install it or set FAFNIRX_RCLONE")
    for b in (SERVER, CLIENT):
        if not b.exists():
            raise SystemExit(f"missing {b}; run cargo build first")
    tmp = Path(tempfile.mkdtemp(prefix="fafnirx-e2e-io-"))
    print("working in", tmp)
    data, port = tmp / "data", free_port()
    db_url, drop_db = pgtest.create()

    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    pem = key.private_bytes(
        serialization.Encoding.PEM, serialization.PrivateFormat.PKCS8, serialization.NoEncryption()
    ).decode()
    drive = FakeDrive(key.public_key())
    sa = {
        "client_email": "bot@proj.iam.gserviceaccount.com",
        "private_key": pem,
        "token_uri": f"http://127.0.0.1:{drive.port}/token",
    }

    e2e.ENV.update(
        FAFNIRX_GDRIVE_API=f"http://127.0.0.1:{drive.port}",
        FAFNIRX_RCLONE=RCLONE,
        FAFNIRX_TICK_SECS="1",
    )
    server = Server(data, port)
    try:
        data.mkdir()
        server.start()
        web = e2e.web_client(data)
        conn = pgtest.connect(db_url)

        def reconstruct(path):
            return e2e.reconstruct(data, conn, path)

        def status(table, name):
            return conn.execute(
                f"SELECT last_status, last_message, last_run_at FROM {table} WHERE name = %s", (name,)
            ).fetchone()

        def run_and_wait(table, name):
            before = status(table, name)[2] or 0
            time.sleep(1.05)  # run stamps have one second resolution
            web.post(f"/{table}/{name}/run")
            row = wait_for(
                lambda: (lambda s: s if s[2] and s[2] > before and s[0] != "running" else None)(status(table, name)),
                f"{table} {name} run",
            )
            return row

        # --- Google Drive source -----------------------------------------------------------
        big = os.urandom(CHUNK * 2 + 777)
        drive.put(FOLDER, "Photos", None, parent="root", mime=FOLDER_MIME)
        drive.put("FILE000001", "a.txt", b"first version")
        drive.put("FILE000002", "big.bin", big)
        drive.put("FILE000003", "dup.txt", b"one")
        drive.put("FILE000004", "dup.txt", b"two")
        drive.put("FILE000005", "Notes", b"", mime=DOC_MIME)
        drive.put("FILE000006", "shortcut", None, mime="application/vnd.google-apps.shortcut")
        drive.put("FILE000007", "con/tains:bad?.txt", b"odd name")
        drive.put(SUBFOLDER, "sub", None, mime=FOLDER_MIME)
        drive.put("FILE000008", "b.txt", b"in a sub folder", parent=SUBFOLDER)
        drive.flaky_once.add("FILE000001")

        r = web.post(
            "/sources",
            data={"name": "drive", "folders": FOLDER, "service_account": json.dumps(sa), "interval_hours": 24},
        )
        check(r.status_code == 303, "google drive source added in the web UI")
        s = wait_for(
            lambda: (lambda s: s if s[0] not in ("running", None) else None)(status("sources", "drive")),
            "first source run",
        )
        check(s[0] == "ok", f"first run succeeds ({s[0]}: {s[1]!r})")
        check(drive.tokens_issued >= 1, "service account assertion was signed and accepted (RS256)")

        dev = conn.execute("SELECT id FROM devices WHERE name = 'source:drive'").fetchone()[0]
        paths = {r[0] for r in db_rows(conn, "SELECT path FROM files WHERE device_id = %s AND deleted_at IS NULL", dev)}
        want = {
            "Photos/a.txt",
            "Photos/big.bin",
            "Photos/dup.txt",
            "Photos/dup (FILE00).txt",
            "Photos/con_tains_bad_.txt",
            "Photos/sub/b.txt",
        }
        check(paths == want, f"namespace holds raw files only, names made safe and unique ({sorted(paths)})")
        check("2 skipped" in s[1], "native docs and shortcuts are skipped and counted")
        check(reconstruct("Photos/big.bin") == big, "multi-chunk Drive file is stored byte for byte")
        check(reconstruct("Photos/a.txt") == b"first version", "a transient 503 from Drive was retried")
        check(web.get("/api/devices").json() == [], "a source never shows up as a device")

        # --- changes in Drive: edit, delete, rename, add ----------------------------------------
        downloads_before = drive.media_requests.count("FILE000002")
        drive.put("FILE000001", "a.txt", b"second version", modified="2024-06-01T10:00:00.000Z")
        del drive.items["FILE000008"]
        drive.items["FILE000002"]["name"] = "big-renamed.bin"
        drive.put("FILE000009", "new.txt", b"fresh")
        s = run_and_wait("sources", "drive")
        check(s[0] == "ok", f"second run succeeds ({s[1]!r})")
        paths = {r[0] for r in db_rows(conn, "SELECT path FROM files WHERE device_id = %s AND deleted_at IS NULL", dev)}
        check(
            "Photos/big-renamed.bin" in paths
            and "Photos/big.bin" not in paths
            and "Photos/new.txt" in paths
            and "Photos/sub/b.txt" not in paths,
            "edit, rename, add and delete in Drive are all picked up",
        )
        check(drive.media_requests.count("FILE000002") == downloads_before, "a renamed file is not downloaded again")
        check(reconstruct("Photos/a.txt") == b"second version", "edited file has the new content")
        old = db_rows(
            conn, "SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'Photos/a.txt'"
        )[0][0]
        check(old == 2, "the previous version is kept")
        check(
            reconstruct("Photos/sub/b.txt") == b"in a sub folder",
            "a file deleted in Drive stays recoverable (retention)",
        )
        ev = db_rows(conn, "SELECT action, detail FROM events WHERE path = 'Photos/big-renamed.bin'")
        check(ev == [("rename", "Photos/big.bin")], "rename is recorded in the change log")

        # --- Drive failure must not look like mass deletion --------------------------------------
        drive.fail_listing = True
        s = run_and_wait("sources", "drive")
        check(s[0] == "failed", f"a failing listing fails the run ({s[1][:80]!r})")
        live = db_rows(conn, "SELECT COUNT(*) FROM files WHERE device_id = %s AND deleted_at IS NULL", dev)[0][0]
        check(live == len(paths), "nothing was deleted because Drive was unreachable")
        check(
            "drive" in web.get("/").text and "Source failed" in web.get("/").text,
            "failed run raises an alert on the home page",
        )
        drive.fail_listing = False
        s = run_and_wait("sources", "drive")
        check(s[0] == "ok" and "Source failed" not in web.get("/").text, "alert clears after a good run")

        # --- a device too, so the mirror has both namespaces ---------------------------------------
        token = re.search(
            r'<pre class="code">([\w-]{40,})</pre>', web.post("/devices", data={"name": "laptop"}).text
        ).group(1)
        cfg = tmp / "client" / "config.toml"
        c = lambda *a: run(CLIENT, "--config", cfg, *a)
        c("enroll", "--server", f"https://127.0.0.1:{port}", "--token", token, "--ca", data / "certs" / "ca.pem")
        cfg.write_text(f'state_dir = "{(tmp / "state").as_posix()}"\n' + cfg.read_text())
        docs = tmp / "docs"
        (docs / "deep").mkdir(parents=True)
        (docs / "x.txt").write_text("from a device")
        (docs / "deep" / "y.txt").write_text("deeper")
        c("add-folder", docs, "--name", "docs")
        c("sync")

        # --- mirror ------------------------------------------------------------------------------
        mirror_dir = tmp / "offsite"
        form = {
            "name": "offsite",
            "remote_type": "local",
            "options": "",
            "remote_path": mirror_dir.as_posix(),
            "interval_hours": 72,
            "keep_runs": 3,
        }
        check(web.post("/mirrors", data=form).status_code == 303, "mirror added in the web UI")
        s = wait_for(
            lambda: (lambda s: s if s[0] not in ("running", None) else None)(status("mirrors", "offsite")),
            "first mirror run",
        )
        check(s[0] == "ok", f"first mirror run succeeds ({s[0]}: {s[1]!r})")
        files = tree(mirror_dir / "files")
        check(
            files["sources/drive/Photos/a.txt"] == b"second version"
            and files["sources/drive/Photos/big-renamed.bin"] == big,
            "mirror holds the current Drive files as a plain tree",
        )
        check(
            files["devices/laptop/docs/x.txt"] == b"from a device"
            and files["devices/laptop/docs/deep/y.txt"] == b"deeper",
            "mirror holds device files under devices/<name>/",
        )
        check(
            "sources/drive/Photos/sub/b.txt" not in files and "sources/drive/Photos/big.bin" not in files,
            "deleted and renamed-away files are not in the current tree",
        )
        stats = json.loads((mirror_dir / "meta" / "stats.json").read_text())
        check(
            stats["files"] == len(files)
            and {n["namespace"] for n in stats["namespaces"]} == {"devices/laptop", "sources/drive"},
            f"stats.json describes the tree ({stats['files']} files)",
        )
        listing = json.loads((mirror_dir / "meta" / "devices_laptop.json").read_text())
        check(
            {f["path"] for f in listing["files"]} == {"docs/x.txt", "docs/deep/y.txt"}
            and all(f["root_hash"] and f["size"] for f in listing["files"]),
            "per-namespace file list is written as JSON",
        )
        sizes_ok = all(
            len(v) == next(f["size"] for f in listing["files"] if f["path"] == k[len("devices/laptop/") :])
            for k, v in files.items()
            if k.startswith("devices/laptop/")
        )
        check(sizes_ok, "file list sizes match the mirrored files")
        check(db_rows(conn, "SELECT last_files FROM mirrors")[0][0] == len(files), "web UI has the mirrored file count")

        # --- incremental run uploads only what changed; replaced and removed files go to versions/ ------
        (docs / "x.txt").write_text("device edit")
        c("sync")
        drive.put("FILE000001", "a.txt", b"third version", modified="2024-07-01T10:00:00.000Z")
        del drive.items["FILE000009"]  # new.txt removed
        run_and_wait("sources", "drive")
        s = run_and_wait("mirrors", "offsite")
        check(
            s[0] == "ok" and s[1].startswith("2 uploaded"),
            f"only the two changed files are uploaded ({s[1].splitlines()[0]!r})",
        )
        files = tree(mirror_dir / "files")
        check(
            files["sources/drive/Photos/a.txt"] == b"third version"
            and files["devices/laptop/docs/x.txt"] == b"device edit",
            "changes reached the mirror",
        )
        check("sources/drive/Photos/new.txt" not in files, "a file deleted at the source is gone from the current tree")
        versions = tree(mirror_dir / "versions")
        kept = {Path(k).relative_to(Path(k).parts[0]).as_posix(): v for k, v in versions.items()}
        check(
            kept.get("sources/drive/Photos/a.txt") == b"second version"
            and kept.get("devices/laptop/docs/x.txt") == b"from a device",
            "replaced files are kept under versions/<run>/",
        )
        check(kept.get("sources/drive/Photos/new.txt") == b"fresh", "removed files are kept under versions/<run>/")

        # --- only the newest runs are kept ---------------------------------------------------------------
        for n in range(5):
            drive.put("FILE000001", "a.txt", f"revision {n}".encode(), modified=f"2024-08-0{n + 1}T10:00:00.000Z")
            run_and_wait("sources", "drive")
            run_and_wait("mirrors", "offsite")
        dirs = sorted(p.name for p in (mirror_dir / "versions").iterdir())
        check(len(dirs) == 3, f"old version runs are pruned to the newest 3 ({dirs})")
        check(len(set(dirs)) == 3 and dirs == sorted(dirs), "version directories are UTC timestamps")

        # --- bit rot in the chunk store is caught before it reaches the mirror ----------------------------
        (h4,) = db_rows(
            conn,
            "SELECT manifest FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'docs/deep/y.txt' ORDER BY v.id DESC LIMIT 1",
        )[0]
        hx = h4[:32].hex()
        chunk = data / "chunks" / hx[:2] / hx[2:4] / hx
        blob = bytearray(chunk.read_bytes())
        blob[0] ^= 1
        chunk.write_bytes(bytes(blob))
        web.post("/mirrors/offsite/resync")
        s = wait_for(
            lambda: (lambda s: s if s[0] not in ("running",) and s[2] and s[1] and "corrupt" in s[1] else None)(
                status("mirrors", "offsite")
            ),
            "corruption report",
        )
        check(
            s[0] == "partial" and "docs/deep/y.txt" in s[1],
            f"corrupt chunk is reported, the rest still mirrors ({s[0]})",
        )
        check("Mirror failed" in web.get("/").text, "partial mirror run raises an alert")
        check(
            tree(mirror_dir / "files").get("devices/laptop/docs/deep/y.txt") == b"deeper",
            "the previously good copy on the mirror was not overwritten with corrupt data",
        )

        # --- a broken target fails loudly ---------------------------------------------------------------------
        web.post("/mirrors", data={**form, "name": "broken", "remote_type": "doesnotexist"})
        s = wait_for(
            lambda: (lambda s: s if s[0] not in ("running", None) else None)(status("mirrors", "broken")),
            "broken mirror run",
        )
        check(s[0] == "failed", f"an unusable mirror fails with a message ({s[1][:90]!r})")

        # --- credentials never leak back ------------------------------------------------------------------------
        page = web.get("/sources").text + web.get("/mirrors").text
        check("PRIVATE KEY" not in page and pem[40:80] not in page, "service account key is never rendered")
        logs = (tmp / "server.log").read_text(errors="replace")
        check("PRIVATE KEY" not in logs and "test-token" not in logs, "credentials and tokens are not logged")

        print(f"\nall {e2e.passed} checks passed")
    finally:
        server.stop()
        drop_db()
        drive.stop()
        if os.environ.get("FAFNIRX_KEEP") != "1":
            shutil.rmtree(tmp, ignore_errors=True)
        else:
            print("kept", tmp)


if __name__ == "__main__":
    main()
