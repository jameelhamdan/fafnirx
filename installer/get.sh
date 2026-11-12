#!/bin/sh
# Installs the fafnirx client on Linux or macOS in one command: downloads the program for this machine, checks its sha256, and
# runs `fafnirx install` with your options. It asks for the administrator password: the client is a service of the device.
#
#   curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.sh | sh -s -- \
#       --server nas.lan --folder ~/Documents
#
# All options go to `fafnirx install` (--server, --folder, --no-start, --dry-run). The device token is asked for on the
# terminal, or taken from FAFNIRX_TOKEN. FAFNIRX_VERSION=4.0.4 installs that version instead of the latest.
set -eu

repo=jameelhamdan/fafnirx

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) echo "this installer is for Linux and macOS; on Windows use get.ps1 or the setup from the release page" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) echo "no build for $(uname -m)" >&2; exit 1 ;;
esac

version=${FAFNIRX_VERSION:-}
if [ -z "$version" ]; then
  # The releases/latest page redirects to .../tag/v<version>.
  latest=$(curl -fsSIL -o /dev/null -w '%{url_effective}' "https://github.com/$repo/releases/latest")
  version=${latest##*/v}
fi
[ -n "$version" ] || { echo "cannot find the latest version; set FAFNIRX_VERSION" >&2; exit 1; }

name=fafnirx-$version-$os-$arch
url=https://github.com/$repo/releases/download/v$version
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

echo "downloading $name"
curl -fsSL -o "$work/fafnirx" "$url/$name"
curl -fsSL -o "$work/$name.sha256" "$url/$name.sha256"

expected=$(cut -d ' ' -f 1 "$work/$name.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$work/fafnirx" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$work/fafnirx" | cut -d ' ' -f 1)
fi
[ "$expected" = "$actual" ] || { echo "the download does not match its sha256; nothing was installed" >&2; exit 1; }
chmod +x "$work/fafnirx"

# When this script is piped into sh, standard input is the script: the token prompt must read the terminal.
if [ -z "${FAFNIRX_TOKEN:-}" ] && [ -r /dev/tty ]; then
  "$work/fafnirx" install "$@" </dev/tty
else
  "$work/fafnirx" install "$@"
fi
