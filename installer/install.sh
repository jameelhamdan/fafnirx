#!/bin/sh
# Installs the fafnirx client on Linux or macOS as a service of the computer: it starts at boot, for every user, and its tray
# starts when a user logs in (a systemd unit on Linux, a LaunchDaemon on macOS). It copies the binary to /usr/local/bin, enrolls
# this device with the master, adds the folders to collect, and starts the service. It asks for the administrator password.
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
    -h | --help) sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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

# Everything that changes the computer is done as root. The token goes to enroll through the environment, not the command line.
if [ "$(id -u)" = 0 ]; then root=; else
  command -v sudo >/dev/null 2>&1 || { echo "this installs a service: run it as root, or install sudo" >&2; exit 1; }
  root="sudo --preserve-env=FAFNIRX_TOKEN"
fi

dest=/usr/local/bin
# The service runs as root, so its program must be one that users cannot change: owned by root, in a folder that is.
# `install` replaces the file by a new one, so a running client keeps its old file until it restarts.
$root mkdir -p "$dest"
$root install -m 755 "$binary" "$dest/fafnirx"
echo "installed $dest/fafnirx"

# enroll asks for the token itself (hidden prompt, FAFNIRX_TOKEN, or standard input), makes the config of the computer and
# installs and starts the service.
$root "$dest/fafnirx" enroll --server "$server"
# Folders are added after enrolling; the running client picks them up at once.
printf '%s\n' "$folders" | while IFS= read -r folder; do
  if [ -n "$folder" ]; then $root "$dest/fafnirx" add-folder "$folder"; fi
done
echo "done: fafnirx runs in the background now. Check it with: fafnirx status"
echo "restart it with: fafnirx restart    update it with: fafnirx update"
