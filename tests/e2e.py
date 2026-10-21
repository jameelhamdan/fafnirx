"""End-to-end test: real fafnirx-server, real web UI, real client binaries.

Run from the repo root with the web venv's python:
    web/.venv/Scripts/python tests/e2e.py [bin_dir]
bin_dir defaults to $FAFNIRX_BIN_DIR or target/debug.
"""

import contextlib
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pgtest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "web"))

EXE = ".exe" if os.name == "nt" else ""
BIN = Path(sys.argv[1] if len(sys.argv) > 1 else os.environ.get("FAFNIRX_BIN_DIR", ROOT / "target" / "debug"))
SERVER, CLIENT, SIGN = (BIN / f"{n}{EXE}" for n in ("fafnirx-server", "fafnirx", "fafnirx-sign"))
CHUNK = 4 * 1024 * 1024
ENV = {**os.environ, "FAFNIRX_TOKEN_STORE": "file", "RUST_LOG": "info"}

passed = 0


def check(cond, msg):
    global passed
    if not cond:
        raise SystemExit(f"FAIL: {msg}")
    passed += 1
    print(f"  ok  {msg}")


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def run(*args, expect_ok=True, timeout=120, **kw):
    r = subprocess.run([str(a) for a in args], capture_output=True, text=True, env=ENV, timeout=timeout, **kw)
    if expect_ok and r.returncode != 0:
        raise SystemExit(f"command failed: {args}\n{r.stdout}\n{r.stderr}")
    return r


class Server:
    def __init__(self, data: Path, port: int):
        self.data, self.port, self.proc = data, port, None

    def start(self):
        env = {
            **ENV,
            "FAFNIRX_DATABASE_URL": os.environ["FAFNIRX_DATABASE_URL"],
            "FAFNIRX_DATA": str(self.data),
            "FAFNIRX_LISTEN": f"127.0.0.1:{self.port}",
        }
        self.log = open(self.data.parent / "server.log", "ab")  # noqa: SIM115 (closed in stop)
        self.proc = subprocess.Popen([str(SERVER)], env=env, stdout=self.log, stderr=self.log)
        for _ in range(100):
            if (self.data / "certs" / "server.pem").exists():
                try:
                    socket.create_connection(("127.0.0.1", self.port), timeout=0.2).close()
                    return
                except OSError:
                    pass
            time.sleep(0.1)
        raise SystemExit("server did not start")

    def stop(self):
        if self.proc:
            self.proc.terminate()
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            self.proc = None


def reconstruct(data: Path, conn, path: str) -> bytes:
    """Rebuilds a file purely from the orchestrator's chunk store."""
    manifest, size = conn.execute(
        "SELECT v.manifest, v.size FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.path = %s",
        (path,),
    ).fetchone()
    out = b""
    for i in range(0, len(manifest), 32):
        h = manifest[i : i + 32].hex()
        out += (data / "chunks" / h[:2] / h[2:4] / h).read_bytes()
    assert len(out) == size, (len(out), size)
    return out


def web_client(data: Path):
    """The real web app on the test database, logged in as admin."""
    os.environ["FAFNIRX_DATA"] = str(data)
    os.environ["FAFNIRX_ADMIN_PASSWORD"] = "pw"
    from fafnirx_web.app import create_app
    from fastapi.testclient import TestClient

    web = TestClient(create_app(), follow_redirects=False)
    check(web.post("/login", data={"password": "pw"}).status_code == 303, "web login")
    return web


def main():
    for b in (SERVER, CLIENT, SIGN):
        if not b.exists():
            raise SystemExit(f"missing {b}; run cargo build first")
    tmp = Path(tempfile.mkdtemp(prefix="fafnirx-e2e-"))
    print("working in", tmp)
    data, port = tmp / "data", free_port()
    db_url, drop_db = pgtest.create()
    server = Server(data, port)
    proc = None
    try:
        data.mkdir()
        server.start()

        # --- web UI creates the device -------------------------------------------------
        web = web_client(data)
        r = web.post("/devices", data={"name": "laptop"})
        token = re.search(r'<pre class="code">([\w-]{40,})</pre>', r.text).group(1)
        check(bool(token), "device created in the web UI, token shown once")
        check("BEGIN CERTIFICATE" in web.get("/ca.pem").text, "web UI serves the CA certificate")

        # --- enroll the client ------------------------------------------------------------
        cdir = tmp / "client"
        cfg = cdir / "config.toml"
        c = lambda *a, **k: run(CLIENT, "--config", cfg, *a, **k)
        out = c(
            "enroll", "--server", f"https://127.0.0.1:{port}", "--token", token, "--ca", data / "certs" / "ca.pem"
        ).stdout
        check('as device "laptop"' in out, f"enroll connects over TLS with the pinned CA ({out.strip()!r})")
        # keep the client's state inside the sandbox
        cfg.write_text(f'state_dir = "{(tmp / "state").as_posix()}"\n' + cfg.read_text())

        # --- first collection ----------------------------------------------------------------
        src = tmp / "docs"
        (src / "sub" / "deep").mkdir(parents=True)
        big = os.urandom(CHUNK * 2 + 12345)
        (src / "big.bin").write_bytes(big)
        (src / "sub" / "a.txt").write_text("hello")
        (src / "sub" / "deep" / "b.txt").write_text("world")
        (src / "skip.tmp").write_text("ignored")
        (src / ".fafnirxignore").write_text("*.tmp\n")
        (src / "empty.txt").write_bytes(b"")
        with contextlib.suppress(OSError):  # creating symlinks needs a privilege on Windows
            os.symlink(src / "sub" / "a.txt", src / "link.txt")
        c("add-folder", src, "--name", "docs")
        out = c("sync").stdout
        check("5 uploaded" in out, f"first sync uploads 5 files ({out.strip()!r})")

        conn = pgtest.connect(db_url)
        paths = {r[0] for r in conn.execute("SELECT path FROM files WHERE deleted_at IS NULL")}
        check(
            paths == {"docs/big.bin", "docs/sub/a.txt", "docs/sub/deep/b.txt", "docs/.fafnirxignore", "docs/empty.txt"},
            f"namespace holds the expected paths {sorted(paths)}",
        )
        check(
            reconstruct(data, conn, "docs/big.bin") == big,
            "multi-chunk file reconstructs byte for byte from the chunk store",
        )
        check(reconstruct(data, conn, "docs/sub/a.txt") == b"hello", "small file reconstructs")

        # --- unchanged: nothing is sent -------------------------------------------------------
        out = c("sync").stdout
        check("0 uploaded" in out and "0 bytes sent" in out, "second sync sends nothing")

        # --- edit one chunk of the big file: only that chunk travels ---------------------------
        edited = bytearray(big)
        edited[10] ^= 0xFF
        (src / "big.bin").write_bytes(edited)
        out = c("sync").stdout
        sent = int(re.search(r"(\d+) bytes sent", out).group(1))
        check(sent == CHUNK, f"editing one chunk sends only that chunk ({sent} bytes)")
        versions = conn.execute(
            "SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'docs/big.bin'"
        ).fetchone()[0]
        check(versions == 2, "the old version is kept next to the new one")
        check(reconstruct(data, conn, "docs/big.bin") == bytes(edited), "current version is the edited file")

        # --- dedup across paths ------------------------------------------------------------------
        shutil.copy(src / "big.bin", src / "copy-of-big.bin")
        out = c("sync").stdout
        check("0 bytes sent" in out, "a copy of existing content uploads no data (dedup)")

        # --- rename --------------------------------------------------------------------------------
        (src / "sub" / "a.txt").rename(src / "sub" / "renamed.txt")
        out = c("sync").stdout
        check("1 renamed" in out and "0 bytes sent" in out, f"rename detected, no re-upload ({out.strip()!r})")
        ev = conn.execute("SELECT action, detail FROM events WHERE path = 'docs/sub/renamed.txt'").fetchone()
        check(ev == ("rename", "docs/sub/a.txt"), "rename recorded in the change log")

        # --- delete: data stays ---------------------------------------------------------------------
        (src / "sub" / "deep" / "b.txt").unlink()
        out = c("sync").stdout
        check("1 deleted" in out, "delete reported")
        (deleted_at,) = conn.execute("SELECT deleted_at FROM files WHERE path = 'docs/sub/deep/b.txt'").fetchone()
        check(deleted_at is not None, "path marked deleted")
        check(reconstruct(data, conn, "docs/sub/deep/b.txt") == b"world", "deleted file is still fully recoverable")

        # --- resume after the client is killed mid-upload ----------------------------------------------
        huge = os.urandom(CHUNK * 6)
        (src / "huge.bin").write_bytes(huge)
        chunks_before = sum(1 for f in (data / "chunks").rglob("*") if f.is_file())
        cfg.write_text(cfg.read_text().replace("bandwidth_limit_kbps = 0", "bandwidth_limit_kbps = 2048"))
        p = subprocess.Popen(
            [str(CLIENT), "--config", str(cfg), "sync"], env=ENV, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        time.sleep(5.0)
        p.kill()
        p.wait()
        chunks_partial = sum(1 for f in (data / "chunks").rglob("*") if f.is_file())
        check(
            0 < chunks_partial - chunks_before < 6,
            f"upload was interrupted part way ({chunks_partial - chunks_before} of 6 chunks stored)",
        )
        check(
            conn.execute("SELECT COUNT(*) FROM files WHERE path = 'docs/huge.bin'").fetchone()[0] == 0,
            "killed upload leaves no half-written file visible",
        )
        cfg.write_text(cfg.read_text().replace("bandwidth_limit_kbps = 2048", "bandwidth_limit_kbps = 0"))
        out = c("sync").stdout
        sent = int(re.search(r"(\d+) bytes sent", out).group(1))
        check(sent < len(huge), f"resume re-sends only missing chunks ({sent} of {len(huge)} bytes)")
        check(reconstruct(data, conn, "docs/huge.bin") == huge, "resumed file is complete and identical")

        # --- server down: changes queue, then flow when it returns -----------------------------------------
        server.stop()
        (src / "while-offline.txt").write_text("written while the server was down")
        r = c("sync", expect_ok=False)
        check(
            r.returncode != 0 and "sync failed" in r.stderr,
            "sync reports an error while the orchestrator is unreachable",
        )
        server.start()
        out = c("sync").stdout
        check("1 uploaded" in out, "queued change uploads once the orchestrator is back")

        # --- heartbeat + web stats -----------------------------------------------------------------------------
        live = {d["name"]: d for d in web.get("/api/devices").json()}
        check(
            live["laptop"]["files"] >= 6 and live["laptop"]["collected"] != "0 B",
            "web UI shows collected data for the device",
        )

        # --- daemon: watcher uploads, pause/resume, status ---------------------------------------------------------
        proc = subprocess.Popen(
            [str(CLIENT), "--config", str(cfg), "run"], env=ENV, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
        )
        deadline = time.time() + 30
        while time.time() < deadline and "online" not in "".join(c("status").stdout.split("\n")[:2]):
            time.sleep(0.5)
        check("online" in c("status").stdout, "daemon connects and reports online")
        live = {d["name"]: d for d in web.get("/api/devices").json()}
        check(live["laptop"]["online"] is True, "web UI shows the device online")
        (src / "watched.txt").write_text("picked up by the watcher")
        deadline = time.time() + 30
        while (
            time.time() < deadline
            and not conn.execute("SELECT 1 FROM files WHERE path = 'docs/watched.txt'").fetchone()
        ):
            time.sleep(0.5)
        check(
            conn.execute("SELECT 1 FROM files WHERE path = 'docs/watched.txt'").fetchone() is not None,
            "file watcher uploads a new file",
        )
        c("pause")
        time.sleep(1)
        (src / "while-paused.txt").write_text("not yet")
        time.sleep(6)
        check(
            conn.execute("SELECT 1 FROM files WHERE path = 'docs/while-paused.txt'").fetchone() is None,
            "nothing uploads while paused",
        )
        check("paused" in c("status").stdout, "status shows paused")
        c("resume")
        deadline = time.time() + 30
        while (
            time.time() < deadline
            and not conn.execute("SELECT 1 FROM files WHERE path = 'docs/while-paused.txt'").fetchone()
        ):
            time.sleep(0.5)
        check(
            conn.execute("SELECT 1 FROM files WHERE path = 'docs/while-paused.txt'").fetchone() is not None,
            "upload continues after resume",
        )
        proc.kill()
        proc.wait()
        proc = None

        # --- revoked device is locked out ----------------------------------------------------------------------------------
        (src / "after-revoke.txt").write_text("x")
        web.post("/devices/laptop/revoke")
        r = c("sync", expect_ok=False)
        check(r.returncode != 0, "a revoked device cannot upload")
        check(
            conn.execute("SELECT 1 FROM files WHERE path = 'docs/after-revoke.txt'").fetchone() is None,
            "nothing was stored for the revoked device",
        )

        # --- signed update ------------------------------------------------------------------------------------------------------
        key = tmp / "update.key"
        pub = re.search(r'update_public_key = "([^"]+)"', run(SIGN, "keygen", "--out", key).stdout).group(1)
        new_token = re.search(r'<pre class="code">([\w-]{40,})</pre>', web.post("/devices/laptop/token").text).group(1)
        copy = tmp / f"fafnirx-copy{EXE}"
        shutil.copy(CLIENT, copy)
        before = copy.read_bytes()
        cfg.write_text(f'update_public_key = "{pub}"\n' + cfg.read_text())
        # the copy is "updated" to the signer binary (any other executable works as a payload)
        platform = f"{'windows' if os.name == 'nt' else sys.platform.replace('darwin', 'macos').replace('linux2', 'linux')}-x86_64"
        run(
            SIGN,
            "publish",
            "--key",
            key,
            "--platform",
            platform,
            "--version",
            "99.0.0",
            "--bin",
            SIGN,
            "--out",
            data / "updates",
        )
        run(
            CLIENT,
            "--config",
            cfg,
            "enroll",
            "--server",
            f"https://127.0.0.1:{port}",
            "--token",
            new_token,
            "--ca",
            data / "certs" / "ca.pem",
        )

        # tampered signature must be refused
        sig = data / "updates" / platform / "signature"
        good_sig = sig.read_text()
        flipped = "A" if good_sig[10] != "A" else "B"
        sig.write_text(good_sig[:10] + flipped + good_sig[11:])  # still a well-formed signature, but not ours
        r = run(copy, "--config", cfg, "update", expect_ok=False)
        check(
            r.returncode != 0 and copy.read_bytes() == before,
            f"tampered update is refused and the binary is untouched (rc={r.returncode} {r.stdout.strip()!r} {r.stderr.strip()[-300:]!r})",
        )
        sig.write_text(good_sig)
        r = run(copy, "--config", cfg, "update", expect_ok=False)
        check(
            r.returncode == 0 and copy.read_bytes() == SIGN.read_bytes(),
            f"signed update replaces the executable ({r.stdout.strip()} {r.stderr.strip()[-200:]})",
        )

        print(f"\nall {passed} checks passed")
    finally:
        if proc:
            proc.kill()
        server.stop()
        drop_db()
        if os.environ.get("FAFNIRX_KEEP") != "1":
            shutil.rmtree(tmp, ignore_errors=True)
        else:
            print("kept", tmp)


if __name__ == "__main__":
    main()
