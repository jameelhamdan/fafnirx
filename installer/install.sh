#!/bin/sh
# Installs the fafnirx client for the current user on Linux or macOS: copies the binary, enrolls this
# device with the master, adds the folders to collect, and starts the client in the background and at
# every login (a systemd user unit on Linux, a LaunchAgent on macOS).
#
#   sh install.sh --server http://nas.lan:7443 --folder ~/Documents [--folder ~/Pictures]
#
# The device token is asked for without echo. For scripts, put it in FAFNIRX_TOKEN or pipe it in; it is never an
# argument, which other users of the machine could see.
#
# The binary is taken from --binary, or from `fafnirx` next to this script, or from target/client.
set -eu

server= binary=
folders=
while [ $# -gt 0 ]; do
  case $1 in
    --server) server=$2; shift 2 ;;
    --folder) folders="$folders
$2"; shift 2 ;;
    --binary) binary=$2; shift 2 ;;
    -h | --help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
done
[ -n "$server" ] || { echo "--server is required (see --help)" >&2; exit 2; }

here=$(cd "$(dirname "$0")" && pwd)
if [ -z "$binary" ]; then
  for candidate in "$here/fafnirx" "$here/../target/client/fafnirx"; do
    [ -x "$candidate" ] && binary=$candidate && break
  done
fi
[ -n "$binary" ] && [ -x "$binary" ] || { echo "cannot find the fafnirx binary; pass --binary" >&2; exit 1; }

dest=${HOME}/.local/bin
mkdir -p "$dest"
# Replace through a new name: the running client keeps its old file until it restarts.
cp "$binary" "$dest/fafnirx.new" && chmod 755 "$dest/fafnirx.new" && mv -f "$dest/fafnirx.new" "$dest/fafnirx"
echo "installed $dest/fafnirx"
case ":$PATH:" in *":$dest:"*) ;; *) echo "note: add $dest to your PATH to run fafnirx by hand" ;; esac

# enroll asks for the token itself (hidden prompt, FAFNIRX_TOKEN, or standard input).
"$dest/fafnirx" enroll --server "$server"
# Folders are added after enrolling; the running client picks them up at once.
printf '%s\n' "$folders" | while IFS= read -r folder; do
  if [ -n "$folder" ]; then "$dest/fafnirx" add-folder "$folder"; fi
done
echo "done: fafnirx runs in the background now. Check it with: fafnirx status"
