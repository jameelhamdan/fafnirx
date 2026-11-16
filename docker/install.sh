#!/bin/sh
# Sets up a fafnirx master in the current directory with the pre-built Docker Hub images, then starts it.
#
#   mkdir fafnirx && cd fafnirx
#   curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/docker/install.sh | sh
#   curl -fsSL .../docker/install.sh | sh -s -- --video --fetch --domain fafnirx.example.com
#
# Options:
#   --video            also run the video module (playback, subtitles, poster frames)
#   --fetch            also run the fetch module (web files and feeds as sources)
#   --domain <name>    put Caddy in front with automatic https for <name> (ports 80, 443 and 7443)
#   --version <x.y.z>  pin the images to a release instead of "latest"
#   --no-start         write the files, do not run docker compose
#
# Safe to run again: an existing .env is never overwritten (the options only add what is missing to it).
set -eu

RAW="${FAFNIRX_RAW_URL:-https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/docker}"
profiles=""
domain=""
version=""
start=1

while [ $# -gt 0 ]; do
  case "$1" in
    --video) profiles="${profiles:+$profiles,}video" ;;
    --fetch) profiles="${profiles:+$profiles,}fetch" ;;
    --domain) domain="${2:?--domain needs a name}"; shift ;;
    --version) version="${2:?--version needs a number}"; shift ;;
    --no-start) start=0 ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

need() { command -v "$1" >/dev/null 2>&1 || { echo "$1 is needed and was not found" >&2; exit 1; }; }
need curl
need docker
docker compose version >/dev/null 2>&1 || { echo "docker compose (v2) is needed" >&2; exit 1; }

secret() {
  if command -v openssl >/dev/null 2>&1; then openssl rand -hex 16
  else head -c 16 /dev/urandom | od -An -tx1 | tr -d ' \n'; fi
}

fetch_file() { [ -f "$1" ] || { echo "downloading $1"; curl -fsSL "$RAW/$1" -o "$1"; }; }

fetch_file docker-compose.yml
compose_files="docker-compose.yml"
if [ -n "$domain" ]; then
  fetch_file docker-compose.proxy.yml
  fetch_file Caddyfile
  compose_files="docker-compose.yml:docker-compose.proxy.yml"
fi

admin_password=""
if [ -f .env ]; then
  echo ".env exists: keeping it"
else
  admin_password="$(secret)"
  {
    echo "FAFNIRX_ADMIN_PASSWORD=$admin_password"
    echo "FAFNIRX_DB_PASSWORD=$(secret)"
    echo "FAFNIRX_PLUGIN_SECRET=$(secret)$(secret)"
    echo "FAFNIRX_UID=$(id -u)"
    echo "FAFNIRX_GID=$(id -g)"
    [ -z "$version" ] || {
      echo "FAFNIRX_IMAGE=jameelh99/fafnirx:$version"
      echo "FAFNIRX_VIDEO_IMAGE=jameelh99/fafnirx-video:$version"
      echo "FAFNIRX_FETCH_IMAGE=jameelh99/fafnirx-fetch:$version"
    }
    [ -z "$profiles" ] || echo "COMPOSE_PROFILES=$profiles"
    [ "$compose_files" = "docker-compose.yml" ] || echo "COMPOSE_FILE=$compose_files"
    [ -z "$domain" ] || { echo "FAFNIRX_WEB_ADDRESS=$domain"; echo "FAFNIRX_DEVICE_ADDRESS=$domain:7443"; }
  } > .env
  chmod 600 .env
  echo "wrote .env"
fi

# The containers run as FAFNIRX_UID; folders Docker would create itself belong to root and cannot be written.
uid="$(sed -n 's/^FAFNIRX_UID=//p' .env | tail -n 1)"
mkdir -p data pgdata config video-cache
if [ -n "${uid:-}" ] && [ "$(id -u)" = "0" ]; then
  chown -R "$uid:$(sed -n 's/^FAFNIRX_GID=//p' .env | tail -n 1)" data config video-cache
fi

if [ "$start" = 0 ]; then
  echo "done; start it with: docker compose up -d"
  exit 0
fi

docker compose up -d

port=8080
[ -z "$domain" ] || port=""
host="${domain:-$(hostname -f 2>/dev/null || hostname)}"
echo
if [ -n "$domain" ]; then url="https://$host"; else url="http://$host:$port"; fi
echo "fafnirx is starting at $url"
if [ -n "$admin_password" ]; then
  echo "log in as: admin"
  echo "password:  $admin_password   (also in .env; change it in the web app)"
else
  echo "log in as admin with the password you set in .env"
fi
echo "next: add a device in the web app (Devices -> Add device) and follow the steps"
