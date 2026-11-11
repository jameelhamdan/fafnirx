#!/usr/bin/env bash
# Starts a fafnirx image and checks that it really works. Used by the Docker workflow before anything is pushed; also fine to
# run by hand:
#   .github/scripts/smoke-image.sh fafnirx:local master
#   .github/scripts/smoke-image.sh fafnirx-video:local video
#   .github/scripts/smoke-image.sh fafnirx-fetch:local fetch
#   .github/scripts/smoke-image.sh fafnirx:local connect fafnirx-video:local video
#   .github/scripts/smoke-image.sh fafnirx:local connect fafnirx-fetch:local fetch
#
# master: the programs it needs are there (and ffmpeg is not: that is the video image's), the database migrations apply
#         against a throwaway PostgreSQL, and the web app and its health check answer.
# video:  ffmpeg and the module are there, and the module answers its health check and refuses a request without the token.
# fetch:  the module is there, and the module answers
#         its health check, refuses a request without the token and says which kinds of source it reads.
# connect: the master image with a module image started next to it and announcing itself to it: the master reaches the module,
#         lists it as up, and the web app's files of the module reach the browser. The last two arguments are the module's image
#         and its id (and its port, for a module this repository does not know).
set -euo pipefail

image=${1:?usage: smoke-image.sh <image> [master|video|fetch|connect <module image> <module> [port]]}
kind=${2:-master}
module_image=${3:-}
module=${4:-video}
name=fx-smoke-$$
port=${SMOKE_PORT:-18081}

cleanup() {
  docker rm -f "$name-master" "$name-db" "$name-video" "$name-fetch" "$name-$module" >/dev/null 2>&1 || true
  docker network rm "$name" >/dev/null 2>&1 || true
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
  docker run -d --name "$name-video" -p "127.0.0.1:$port:8090" -e FAFNIRX_PLUGIN_SECRET=smoke-token-0123456789 "$image" >/dev/null
  wait_for "http://127.0.0.1:$port/health" "the video module" "$name-video"
  test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/manifest")" = 401
  curl -fsS -H 'Authorization: Bearer smoke-token-0123456789' "http://127.0.0.1:$port/manifest" | grep -q '"id":"video"'
  ;;
fetch)
  echo "== programs in the image"
  docker run --rm --entrypoint sh "$image" -c 'command -v fafnirx-fetch; test "$(id -u)" = 1000; ! command -v ffmpeg'

  echo "== the module"
  docker run -d --name "$name-fetch" -p "127.0.0.1:$port:8091" -e FAFNIRX_PLUGIN_SECRET=smoke-token-0123456789 "$image" >/dev/null
  wait_for "http://127.0.0.1:$port/health" "the fetch module" "$name-fetch"
  test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/manifest")" = 401
  manifest=$(curl -fsS -H 'Authorization: Bearer smoke-token-0123456789' "http://127.0.0.1:$port/manifest")
  echo "$manifest" | grep -q '"id":"fetch"'
  echo "$manifest" | grep -q '"fetch.web"'
  ;;
master|connect)
  if [ "$kind" = master ]; then
    echo "== programs in the image"
    docker run --rm --entrypoint sh "$image" -c 'rclone version | head -1 | grep -q "^rclone v1\.75\.2$" && test "$(id -u)" = 1000 && ! command -v ffmpeg'
    docker run --rm --entrypoint sh "$image" -c 'for b in fafnirx-master; do command -v $b; done'
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
  extra=()
  if [ "$kind" = connect ]; then
    : "${module_image:?usage: smoke-image.sh <image> connect <module image> <module>}"
    case "$module" in video) module_port=8090 ;; fetch) module_port=8091 ;; *) module_port=${5:?port of module $module} ;; esac
    secret=smoke-token-0123456789
    # The module is started with the master's address and the secret, and announces itself; the master only has the secret.
    docker run -d --name "$name-$module" --network "$name" -e "FAFNIRX_PLUGIN_SECRET=$secret" \
      -e "FAFNIRX_MASTER_URL=http://$name-master:8080" -e "FAFNIRX_PLUGIN_URL=http://$name-$module:$module_port" "$module_image" >/dev/null
    extra=(-e "FAFNIRX_PLUGIN_SECRET=$secret")
  fi
  docker create --name "$name-master" --network "$name" -p "127.0.0.1:$port:8080" \
    -e "FAFNIRX_DATABASE_URL=postgres://postgres:smoke@$name-db/fafnirx" \
    -e FAFNIRX_ADMIN_PASSWORD=smoke-admin-pw "${extra[@]}" "$image" >/dev/null
  docker start "$name-master" >/dev/null
  wait_for "http://127.0.0.1:$port/healthz" "the master" "$name-master"
  test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")" = 200
  docker logs "$name-master" 2>&1 | grep -q 'listening for devices'
  if [ "$kind" = connect ]; then
    echo "== the $module module is connected"
    jar=$(mktemp)
    curl -fsS -c "$jar" -H 'x-requested-with: smoke' -H 'content-type: application/json' \
      -d '{"username":"admin","password":"smoke-admin-pw"}' "http://127.0.0.1:$port/api/login" >/dev/null
    for _ in $(seq 1 30); do
      curl -fsS -b "$jar" "http://127.0.0.1:$port/api/session" | grep -q "\"$module\"" && break
      sleep 2
    done
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/session" | grep -q "\"$module\"" || { docker logs "$name-master" >&2; exit 1; }
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/plugins" | grep -q '"state":"up"'
    curl -fsS -b "$jar" "http://127.0.0.1:$port/api/m/$module/ui/index.js" | grep -q 'export'
    if [ "$module" = fetch ]; then
      # The module says which kind of source it reads.
      curl -fsS -b "$jar" "http://127.0.0.1:$port/api/session" | grep -q 'fetch.web'
    fi
    rm -f "$jar"
  fi
  ;;
*)
  echo "unknown kind $kind: master, video, fetch or connect" >&2
  exit 2
  ;;
esac
echo "== the image works"
