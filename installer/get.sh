#!/bin/sh
# Installs the fafnirx client on Linux or macOS in one command: downloads the release for this machine, checks its
# sha256, and runs install.sh from it (copy the binary, enroll the device, add folders, start at login).
#
#   curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.sh | sh -s -- \
#       --server http://nas.lan:7443 --folder ~/Documents
#
# All options go to install.sh (--server, --folder, ...). The device token is asked for on the terminal, or taken from
# FAFNIRX_TOKEN. FAFNIRX_VERSION=3.0.1 installs that version instead of the latest.
set -eu

repo=jameelhamdan/fafnirx

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) echo "this installer is for Linux and macOS; on Windows use the setup from the release page" >&2; exit 1 ;;
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
curl -fsSL -o "$work/$name.tar.gz" "$url/$name.tar.gz"
curl -fsSL -o "$work/$name.tar.gz.sha256" "$url/$name.tar.gz.sha256"

expected=$(cut -d ' ' -f 1 "$work/$name.tar.gz.sha256")
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$work/$name.tar.gz" | cut -d ' ' -f 1)
else
  actual=$(shasum -a 256 "$work/$name.tar.gz" | cut -d ' ' -f 1)
fi
[ "$expected" = "$actual" ] || { echo "the download does not match its sha256; nothing was installed" >&2; exit 1; }

tar -xzf "$work/$name.tar.gz" -C "$work"

# When this script is piped into sh, standard input is the script: the token prompt must read the terminal.
if [ -z "${FAFNIRX_TOKEN:-}" ] && [ -r /dev/tty ]; then
  sh "$work/$name/install.sh" "$@" </dev/tty
  exit $?
fi
sh "$work/$name/install.sh" "$@"
