#!/usr/bin/env bash
# Starts a fafnirx-master image against a throwaway PostgreSQL and checks that it really works: the programs it needs are
# there, the database migrations apply, and the web app and its health check answer. Used by the Docker workflow before
# anything is pushed; also fine to run by hand:  .github/scripts/smoke-image.sh fafnirx-master:local
set -euo pipefail

image=${1:?usage: smoke-image.sh <image>}
name=fx-smoke-$$
port=${SMOKE_PORT:-18081}

cleanup() {
  docker rm -f "$name-master" "$name-db" >/dev/null 2>&1 || true
  docker network rm "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "== programs in the image"
docker run --rm --entrypoint sh "$image" -c 'ffmpeg -version | head -1; ffprobe -version | head -1; rclone version | head -1; test "$(id -u)" = 1000'
docker run --rm --entrypoint sh "$image" -c 'for b in fafnirx-master fafnirx-sign fafnirx-stream; do command -v $b; done'

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
docker run -d --name "$name-master" --network "$name" -p "127.0.0.1:$port:8080" \
  -e "FAFNIRX_DATABASE_URL=postgres://postgres:smoke@$name-db/fafnirx" \
  -e FAFNIRX_ADMIN_PASSWORD=smoke-admin-pw "$image" >/dev/null
ok=
for _ in $(seq 1 60); do
  if curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1; then ok=1; break; fi
  sleep 2
done
if [ -z "$ok" ]; then
  echo "the master did not become healthy; its log:" >&2
  docker logs "$name-master" >&2 || true
  exit 1
fi
test "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$port/")" = 200
docker logs "$name-master" 2>&1 | grep -q 'listening for devices'
echo "== the image works"
