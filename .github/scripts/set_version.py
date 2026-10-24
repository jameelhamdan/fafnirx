#!/usr/bin/env python3
"""Sets the version of every crate of the workspace: the one in Cargo.toml ([workspace.package]) and the entries of the
workspace's own crates in Cargo.lock. Nothing else in either file is touched, byte for byte, and no network is needed, so
the result is exactly what `cargo build --locked` expects.

    set_version.py 1.2.3 [root-of-the-repository]

It stops, without writing anything, if the lockfile does not have an entry for every member of the workspace.
"""
import re
import sys
from pathlib import Path


def set_workspace_version(text: str, version: str) -> str:
    lines = text.split("\n")
    in_section = False
    for i, line in enumerate(lines):
        if line.startswith("["):
            in_section = line.strip() == "[workspace.package]"
        elif in_section and re.match(r"version\s*=", line):
            lines[i] = f'version = "{version}"'
            return "\n".join(lines)
    raise SystemExit("Cargo.toml has no version under [workspace.package]")


def set_lock_versions(text: str, version: str) -> tuple[str, int]:
    """The packages of the lockfile that are crates of the workspace have no `source` (the ones from a registry do)."""
    blocks = text.split("\n\n")
    changed = 0
    for i, block in enumerate(blocks):
        if block.startswith("[[package]]") and "\nsource = " not in block:
            blocks[i], n = re.subn(r'\nversion = "[^"]*"', f'\nversion = "{version}"', block, count=1)
            changed += n
    return "\n\n".join(blocks), changed


def main() -> None:
    if len(sys.argv) not in (2, 3) or not re.fullmatch(r"\d+\.\d+\.\d+", sys.argv[1]):
        raise SystemExit("usage: set_version.py X.Y.Z [root]")
    version = sys.argv[1]
    root = Path(sys.argv[2] if len(sys.argv) == 3 else Path(__file__).resolve().parents[2])
    manifest, lockfile = root / "Cargo.toml", root / "Cargo.lock"

    toml = manifest.read_text(encoding="utf-8", newline="")
    members = re.search(r"^members\s*=\s*\[(.*?)\]", toml, re.S | re.M)
    wanted = len(re.findall(r'"[^"]+"', members.group(1))) if members else 0
    new_toml = set_workspace_version(toml, version)
    new_lock, found = set_lock_versions(lockfile.read_text(encoding="utf-8", newline=""), version)
    if wanted == 0 or found != wanted:
        raise SystemExit(f"Cargo.lock has {found} entries for the {wanted} crates of the workspace: refusing to guess")
    manifest.write_text(new_toml, encoding="utf-8", newline="")
    lockfile.write_text(new_lock, encoding="utf-8", newline="")
    print(f"version {version} in Cargo.toml and for {found} crates in Cargo.lock")


if __name__ == "__main__":
    main()
