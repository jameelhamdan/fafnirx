#!/usr/bin/env bash
# Disk and PostgreSQL I/O in Docker: VM disk vs Mac bind mount, PostgreSQL on 1-3 cores. SKIP_PG=1 skips pgbench.
#   bash bench-disk.sh [bind-folder]
set -euo pipefail
BIND=${1:-$PWD/.bench-bind}; mkdir -p "$BIND"/{dd,pg}
docker volume rm -f fxb-vol >/dev/null 2>&1 || true; docker volume create fxb-vol >/dev/null

echo "== sequential write 512 MiB with fsync at the end, then 1000 small files each fsynced (one core)"
for target in "named volume:-v fxb-vol:/t" "bind mount:-v $BIND/dd:/t"; do
  label=${target%%:*}; mount=${target#*:}
  docker run --rm --cpuset-cpus=0 $mount postgres:17 bash -c '
    s=$(date +%s.%N); dd if=/dev/zero of=/t/big bs=1M count=512 conv=fsync 2>/dev/null; e=$(date +%s.%N)
    echo "  seq write: $(awk "BEGIN{printf \"%d\", 512/($e-$s)}") MiB/s"
    s=$(date +%s.%N); for i in $(seq 1000); do head -c 4096 /dev/zero > /t/s$i; sync -d /t/s$i; done; e=$(date +%s.%N)
    echo "  small fsynced files: $(awk "BEGIN{printf \"%d\", 1000/($e-$s)}") files/s"
    s=$(date +%s.%N); dd if=/t/big of=/dev/null bs=1M 2>/dev/null; e=$(date +%s.%N)
    echo "  seq read (cached): $(awk "BEGIN{printf \"%d\", 512/($e-$s)}") MiB/s"; rm -rf /t/*' | sed "s/^/[$label]/"
done

[[ -n "${SKIP_PG:-}" ]] && { docker volume rm -f fxb-vol >/dev/null; rm -rf "$BIND"; exit 0; }
echo "== pgbench (scale 20, 8 clients), TPC-B-like writes and select-only"
run() { # label cpus volume extra-postgres-args
  docker rm -f fxb-pg >/dev/null 2>&1 || true
  [[ "$3" == fxb-pg-vol ]] && { docker volume rm -f fxb-pg-vol >/dev/null; docker volume create fxb-pg-vol >/dev/null; } || { rm -rf "$BIND/pg"; mkdir -p "$BIND/pg"; }
  mount=$([[ "$3" == fxb-pg-vol ]] && echo "fxb-pg-vol:/var/lib/postgresql/data" || echo "$BIND/pg:/var/lib/postgresql/data")
  docker run -d --name fxb-pg --cpuset-cpus="$2" -e POSTGRES_PASSWORD=x -v "$mount" postgres:17 $4 >/dev/null
  until docker exec fxb-pg pg_isready -U postgres -q; do sleep 1; done; sleep 2
  docker exec fxb-pg pgbench -U postgres -i -s 20 -q postgres >/dev/null 2>&1
  w=$(docker exec fxb-pg pgbench -U postgres -c 8 -j 2 -T 15 postgres 2>&1 | awk '/^tps/{print $3}')
  r=$(docker exec fxb-pg pgbench -U postgres -S -c 8 -j 2 -T 10 postgres 2>&1 | awk '/^tps/{print $3}')
  printf "  %-46s write tps %8s   select-only tps %8s\n" "$1" "$w" "$r"
  docker rm -f fxb-pg >/dev/null
}
run "1 core, named volume"                       3   fxb-pg-vol ""
run "2 cores, named volume"                      2-3 fxb-pg-vol ""
run "3 cores, named volume"                      1-3 fxb-pg-vol ""
run "1 core, bind mount"                         3   bind ""
run "1 core, named volume, synchronous_commit=off" 3 fxb-pg-vol "-c synchronous_commit=off"
run "1 core, named volume, fsync=off (ceiling)"  3   fxb-pg-vol "-c fsync=off"
docker volume rm -f fxb-vol fxb-pg-vol >/dev/null; rm -rf "$BIND"
