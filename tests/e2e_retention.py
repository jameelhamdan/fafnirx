"""End-to-end test of the delete strategy.

The client is the authority: a file deleted on the device is only marked deleted on the orchestrator.
Its data stays recoverable for the retention period, then the file, its versions and (once no other file
uses them) its chunks are removed for good.

    web/.venv/Scripts/python tests/e2e_retention.py [bin_dir]

The engine's cleanup job first runs 30 s after start, so this test takes about two minutes.
"""

import os
import re
import shutil
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import e2e  # noqa: E402
import pgtest  # noqa: E402
from e2e import CLIENT, Server, check, free_port, reconstruct, run  # noqa: E402

DAY = 86400


def chunk_paths(data: Path, conn, path: str) -> list[Path]:
    """Chunk files used by every version of a path."""
    paths = []
    for (manifest,) in conn.execute(
        "SELECT v.manifest FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = %s", (path,)
    ):
        for i in range(0, len(manifest), 32):
            h = manifest[i : i + 32].hex()
            paths.append(data / "chunks" / h[:2] / h[2:4] / h)
    return paths


def main():
    tmp = Path(tempfile.mkdtemp(prefix="fafnirx-e2e-ret-"))
    print("working in", tmp)
    data, port = tmp / "data", free_port()
    db_url, drop_db = pgtest.create()
    server = Server(data, port)
    try:
        data.mkdir()
        server.start()
        web = e2e.web_client(data)
        token = re.search(
            r'<pre class="code">([\w-]{40,})</pre>', web.post("/devices", data={"name": "laptop"}).text
        ).group(1)
        cfg = tmp / "client" / "config.toml"
        c = lambda *a: run(CLIENT, "--config", cfg, *a)
        c("enroll", "--server", f"https://127.0.0.1:{port}", "--token", token, "--ca", data / "certs" / "ca.pem")
        cfg.write_text(f'state_dir = "{(tmp / "state").as_posix()}"\n' + cfg.read_text())
        src = tmp / "docs"
        src.mkdir()
        c("add-folder", src)

        (src / "gone.txt").write_bytes(os.urandom(5000))
        original = (src / "gone.txt").read_bytes()
        (src / "keep.txt").write_bytes(os.urandom(5000))
        (src / "shared.txt").write_bytes(b"identical content in two files")
        (src / "shared-copy.txt").write_bytes(b"identical content in two files")
        c("sync")
        conn = pgtest.connect(db_url)
        row = lambda p: conn.execute("SELECT deleted_at FROM files WHERE path = %s", (f"docs/{p}",)).fetchone()
        check(row("gone.txt") == (None,), "file collected and live")

        # --- the client deletes: the orchestrator only marks it ------------------------------------------
        (src / "gone.txt").unlink()
        c("sync")
        check(row("gone.txt")[0] is not None, "deleting on the client marks the file deleted on the orchestrator")
        check(reconstruct(data, conn, "docs/gone.txt") == original, "its data is still fully recoverable")
        check(
            len(chunk_paths(data, conn, "docs/gone.txt")) > 0
            and all(p.exists() for p in chunk_paths(data, conn, "docs/gone.txt")),
            "its chunks are still stored",
        )

        # --- the client wins again if the file comes back ----------------------------------------------------
        (src / "gone.txt").write_bytes(b"recreated on the client")
        c("sync")
        check(row("gone.txt") == (None,), "recreating the file on the client revives it")
        versions = conn.execute(
            "SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'docs/gone.txt'"
        ).fetchone()[0]
        check(versions == 2, "the old content is kept as an earlier version")
        (src / "gone.txt").unlink()
        (src / "shared-copy.txt").unlink()
        c("sync")
        check(row("gone.txt")[0] is not None and row("shared-copy.txt")[0] is not None, "both deletions are marked")

        gone_chunks = chunk_paths(data, conn, "docs/gone.txt")
        shared_chunks = chunk_paths(data, conn, "docs/shared-copy.txt")
        keep_chunks = chunk_paths(data, conn, "docs/keep.txt")
        old = time.time() - 3 * DAY  # chunks must be older than the collection grace period
        for p in gone_chunks + shared_chunks + keep_chunks:
            os.utime(p, (old, old))

        def restart_and_wait_for_cleanup():
            conn.close()
            server.stop()
            server.start()
            time.sleep(36)  # the cleanup job runs 30 s after start
            return pgtest.connect(db_url)

        # --- inside the retention period nothing is removed ---------------------------------------------------
        conn.execute("UPDATE files SET deleted_at = %s WHERE deleted_at IS NOT NULL", (int(time.time()) - 29 * DAY,))
        conn.commit()
        conn = restart_and_wait_for_cleanup()
        row = lambda p: conn.execute("SELECT deleted_at FROM files WHERE path = %s", (f"docs/{p}",)).fetchone()
        check(
            row("gone.txt") is not None and all(p.exists() for p in gone_chunks),
            "29 days after the delete the data is still there",
        )

        # --- after the retention period it is removed for good -------------------------------------------------
        conn.execute("UPDATE files SET deleted_at = %s WHERE deleted_at IS NOT NULL", (int(time.time()) - 31 * DAY,))
        conn.commit()
        conn = restart_and_wait_for_cleanup()
        row = lambda p: conn.execute("SELECT deleted_at FROM files WHERE path = %s", (f"docs/{p}",)).fetchone()
        check(
            row("gone.txt") is None and row("shared-copy.txt") is None,
            "31 days after the delete the file records are gone",
        )
        left = conn.execute(
            "SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path IN ('docs/gone.txt', 'docs/shared-copy.txt')"
        ).fetchone()[0]
        check(left == 0, "and so are all their versions")
        check(not any(p.exists() for p in gone_chunks), "chunks nothing else uses are deleted from disk")
        check(all(p.exists() for p in shared_chunks), "chunks still used by another file (identical content) are kept")
        check(
            row("keep.txt") == (None,) and all(p.exists() for p in keep_chunks),
            "files that were never deleted are untouched",
        )
        check(
            reconstruct(data, conn, "docs/keep.txt") == (tmp / "docs" / "keep.txt").read_bytes(),
            "and still reconstruct byte for byte",
        )

        print(f"\nall {e2e.passed} checks passed")
    finally:
        server.stop()
        drop_db()
        if os.environ.get("FAFNIRX_KEEP") != "1":
            shutil.rmtree(tmp, ignore_errors=True)
        else:
            print("kept", tmp)


if __name__ == "__main__":
    main()
