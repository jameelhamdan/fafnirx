#!/usr/bin/env bash
# Starts a fafnirx image and checks that it really works. Used by the Docker workflow before anything is pushed; also fine to
# run by hand:
#   .github/scripts/smoke-image.sh fafnirx:local master
#   .github/scripts/smoke-image.sh fafnirx-video:local video
#   .github/scripts/smoke-image.sh fafnirx:local plugin out/fafnirx-plugin-video-<version>-linux-x86_64.zip
#
# master: the programs it needs are there (and ffmpeg is not: that is the video image's), the database migrations apply
#         against a throwaway PostgreSQL, and the web app and its health check answer.
# video:  ffmpeg and the module are there, and the module answers its health check and refuses a request without the token.
# plugin: the master image with a plugin bundle unpacked into its data folder: the master starts the plugin, it answers as a module,
#         and the web app's files of the module reach the browser (so the program runs on the image's own system).
set -euo pipefail

image=${1:?usage: smoke-image.sh <image> [master|video|plugin <bundle.zip>]}
kind=${2:-master}
name=fx-smoke-$$
port=${SMOKE_PORT:-18081}

unpacked=
cleanup() {
  docker rm -f "$name-master" "$name-db" "$name-video" >/dev/null 2>&1 || true
  docker network rm "$name" >/dev/null 2>&1 || true
  if [ -n "$unpacked" ]; then rm -rf "$unpacked"; fi
}
trap cleanup EXIT

wait_for() { # <url> <what> <container>
  for _ in $(seq 1 60); do
    if curl -fsS "$1" >/dev/null 2>&1; then return 0; fi
    sleep 2
  done
  echo "$2 did not become healthy; its log:" >&2
  docker logs "$3" >&2 || true
  exit 1
}

case "$kind" in
video)
  echo "== programs in the image"
  docker run --rm --entrypoint sh "$image" -c 'ffmpeg -version | head -1; ffprobe -version | head -1; command -v fafnirx-video; test "$(id -u)" = 1000'

  echo "== the module"
  docker run -d --name "$name-video" -p "127.0.0.1:$port:8090" -e FAFNIRX_MODULE_TOKEN=smoke-token-0123456789 "$image" >/dev/null
  wait_for "http://127.0.0.1:$port/health" "the video module" "$name-video"
  test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/manifest")" = 401
  curl -fsS -H 'Authorization: Bearer smoke-token-0123456789' "http://127.0.0.1:$port/manifest" | grep -q '"id":"video"'
  ;;
master|plugin)
  if [ "$kind" = master ]; then
    echo "== programs in the image"
    docker run --rm --entrypoint sh "$image" -c 'rclone version | head -1; test "$(id -u)" = 1000; ! command -v ffmpeg'
    docker run --rm --entrypoint sh "$image" -c 'for b in fafnirx-master fafnirx-sign; do command -v $b; done'
  fi

  echo "== database"
  docker network create "$name" >/dev/null
  docker run -d --name "$name-db" --network "$name" -e POSTGRES_PASSWORD=smoke -e POSTGRES_DB=fafnirx postgres:17 >/dev/null
  # Over TCP: while the image sets itself up, PostgreSQL answers on its socket only, so a socket check is too early.
  for _ in $(seq 1 60); do
    docker exec "$name-db" psql -h 127.0.0.1 -U postgres -d fafnirx -c 'select 1' >/dev/null 2>&1 && break
    sleep 2
  done
  docker exec "$name-db" psql -h 127.0.0.1 -U postgres -d fafnirx -c 'select 1' >/dev/null

  echo "== master"
  docker create --name "$name-master" --network "$name" -p "127.0.0.1:$port:8080" \
    -e "FAFNIRX_DATABASE_URL=postgres://postgres:smoke@$name-db/fafnirx" \
    -e FAFNIRX_ADMIN_PASSWORD=smoke-admin-pw "$image" >/dev/null
  if [ "$kind" = plugin ]; then
    bundle=${3:?usage: smoke-image.sh <image> plugin <bundle.zip>}
    unpacked=$(mktemp -d)
    mkdir -p "$unpacked/plugins/video"
    unzip -q "$bundle" -d "$unpacked/plugins/video"
    docker cp "$unpacked/plugins" "$name-master:/data/"
  fi
  docker start "$name-master" >/dev/null
  wait_for "http://127.0.0.1:$port/healthz" "the master" "$name-master"
  test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")" = 200
  docker logs "$name-master" 2>&1 | grep -q 'listening for devices'
  if [ "$kind" = plugin ]; then
    echo "== plugin"
    jar=$(mktemp)
    curl -fsS -c "$jar" -H 'x-requested-with: smoke' -H 'content-type: application/json' \
      -d '{"username":"admin","password":"smoke-admin-pw"}' "http://127.0.0.1:$port/api/login" >/dev/null
    for _ in $(seq 1 30); do
      curl -fsS -b "$jar" "http://127.0.0.1:$port/api/session" | grep -q '"video"' && break
      sleep 2
    done
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/session" | grep -q '"video"' || { docker logs "$name-master" >&2; exit 1; }
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/plugins" | grep -q '"state":"up"'
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/m/video/ui/index.js" | grep -q 'export'
    rm -f "$jar"
  fi
  ;;
*)
  echo "unknown kind $kind: master, video or plugin" >&2
  exit 2
  ;;
esac
echo "== the image works"
