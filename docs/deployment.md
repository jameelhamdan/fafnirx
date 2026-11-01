# Deployment

Inbound: `7443` (devices, token) and `8080` (users, password), both plain HTTP: trusted network/VPN, or TLS in front.
Outbound: the storage of your sources and mirrors, only.

## Storage

| What | Default path | Put it on |
|---|---|---|
| PostgreSQL | `./pgdata` (`FAFNIRX_DB_DIR`) | Local disk. Never SMB/NFS (needs locking, `fsync`) |
| Files, versions | `./data` (`FAFNIRX_DATA_DIR`) | A checksumming filesystem (ZFS) |

```text
files/users/<user>/<device>/<path>  current tree; shareable as a network drive
files/<path>                        master files
versions/<id>/<user>/<device>/<path> replaced or deleted versions
tmp/                                uploads being assembled (removed after a day untouched)
cache/thumbs/                       thumbnails and previews, made on demand and kept
plugins/<id>/                       plugins (FAFNIRX_PLUGINS installs them; not user data)
updates/<platform>/                 your own signed client builds (optional)
```

`FAFNIRX_CONFIG_DIR` (`./config`) holds only `rclone.conf`. fafnirx verifies hashes on upload and before mirroring but does
not scrub stored files: scrub the filesystem. ZFS example (mirrored vdev or better, so scrubs repair):

```sh
zfs create -o compression=lz4 -o atime=off -o recordsize=16K tank/fafnirx/pgdata
zfs create -o compression=lz4 -o atime=off tank/fafnirx/data
chown 1000:1000 /tank/fafnirx/pgdata /tank/fafnirx/data
```

**Network drive.** Share `<data>/files` over SMB/NFS, writable for `FAFNIRX_UID`. The policy: master files (everything
outside `files/users/`) are writable over SMB; `files/users/`, which holds every user's folder, is read-only there (share logins
are not fafnirx users, and a device's files are only changed by the device). Never share `versions/`, `tmp/` or `cache/`.
`deploy/samba/` has a ready Samba container: one writable share with `files/users` mounted read-only on top of it, one line
whatever the users. A server that has the older layout (`files/<user>/`) needs the folders moved into `files/users/` once, by hand (`mv`).

```sh
cd deploy/samba && cp .env.example .env     # set SAMBA_PASSWORD and SAMBA_INTERFACE
docker compose up -d
```

## Reverse proxy with Caddy

`docker-compose.proxy.yml` puts [Caddy](https://caddyserver.com) in front and closes the master's ports:

```sh
docker compose -f docker-compose.yml -f docker-compose.proxy.yml up -d     # or COMPOSE_FILE=... in .env
```

Default: plain HTTP on web `80`, devices `7443`. For https set domains (ports 80/443 must reach the host; Caddy handles the
certificate):

```ini
FAFNIRX_WEB_ADDRESS=fafnirx.example.com
FAFNIRX_DEVICE_ADDRESS=fafnirx.example.com:7443
```

Enroll with `fafnirx enroll --server https://fafnirx.example.com:7443`. Host ports: `FAFNIRX_HTTP_PORT`, `FAFNIRX_HTTPS_PORT`,
`FAFNIRX_DEVICE_PORT`. The master trusts Caddy's forwarded address (`FAFNIRX_TRUSTED_PROXY` = the compose network), so logins
are limited per person. The `Caddyfile` is short and editable. Streaming segments are not cached (that would bypass the login).

## Cloudflare tunnel

No inbound port. Two hostnames:

```yaml
ingress:
  - hostname: files.example.com        # web app
    service: http://localhost:8080
  - hostname: devices.example.com      # device API (gRPC)
    service: http://localhost:7443
    originRequest:
      http2Origin: true
  - service: http_status:404
```

Turn on **gRPC** for the zone; put **Cloudflare Access** on `files.` only (devices use tokens); enroll with
`https://devices.example.com`. The cookie is `Secure` when `X-Forwarded-Proto: https` arrives. A tunnel alone makes every
visitor one address (five wrong guesses lock the login for a minute for all): route `cloudflared` through the Caddy proxy
(`http://proxy:80`, `http://proxy:7443`) or set `FAFNIRX_TRUSTED_PROXY` to its address. Responses carry CSP, `X-Frame-Options:
DENY`, `nosniff`, and HSTS behind https. Browser uploads use 8 MiB pieces; live updates use SSE with a 15 s keep-alive. A VPN is equally safe.

## Backups

| Back up | How |
|---|---|
| PostgreSQL (paths → versions, tokens, settings, credentials, library data) | `docker compose exec db pg_dump -U fafnirx fafnirx > fafnirx.sql`, or ZFS snapshots |
| `data/files/`, `data/versions/` | ZFS snapshots, or the offsite mirror |
| `config/rclone.conf` (credentials) | Copy it somewhere private |
| Update signing key | Wherever you keep `update.key` (not on the master) |

Snapshot **the database first, then the data**: a file a newer snapshot lacks is still in `versions/` until retention.
3-2-1: the master is one copy, a snapshot or other disk the second, a mirror the offsite one. A mirror holds files, not the
database; a full restore needs both.

```sh
docker compose exec -T db psql -U fafnirx fafnirx < fafnirx.sql      # into an empty database
```

## Upgrades

`docker compose pull && docker compose up -d`. Missing migrations run on start (in order, one transaction, advisory lock); a
newer database is refused. Back up first; migration 10 gave files, albums, tags, jobs and alerts new UUID ids and cannot be rolled back. A PostgreSQL
major upgrade needs dump and restore. Clients update themselves from GitHub within about six hours.

## Security checklist

- Strong `FAFNIRX_ADMIN_PASSWORD` and `FAFNIRX_DB_PASSWORD`; `.env` readable only by you; PostgreSQL port unpublished.
- `8080` and `7443` off the internet unless behind TLS.
- Device tokens are shown once and stored as SHA-256 hashes; revoke lost devices. A device adds data to its own device; it reads and writes only its user's shared folders.
- Source and mirror credentials, and the encryption key of backup mirrors, are plain text in the database and `rclone.conf`: protect backups and the config folder. Keep a copy of each backup key away from the master.
- Keep the update signing key off the master.
- The master's disk is not encrypted by fafnirx: use an encrypted dataset. A **backup** mirror is encrypted with its own key ([Mirrors](mirrors.md)); a replica is not.

## Sizing and tuning

Measured with `tests/bench_docker.py` and `tests/bench-disk.sh` in Docker on an Apple M2, with the master and PostgreSQL on the
same cores, 5,150 photos and 16 clients. Each figure is one run; ignore differences under 15%.

| Machine | Settings | Timeline | Dashboard | Thumbnails |
|---|---|---|---|---|
| 1 core, 512 MB in all | `FAFNIRX_DB_POOL=4`, `FAFNIRX_JOB_WORKERS=1` | 95 req/s | 391 req/s | 15/s |
| 2 cores, 1 GB in all | `FAFNIRX_DB_POOL=4`, `FAFNIRX_JOB_WORKERS=2` | 179 req/s | 686 req/s | 33/s |
| 4 cores | defaults | 316 req/s | 857 req/s | 61/s |

Neither small machine ran out of memory. The master peaked at 120-200 MiB and PostgreSQL at 60-80 MiB. Transfers of one large
file did not depend on cores (upload about 100 MiB/s, download about 250 MiB/s). Uploading many small files did: 216, 329 and 405
files/s on 1, 2 and 4 cores.

What limits it, first to last:

1. **PostgreSQL CPU.** It uses about 8 times the master's CPU on the timeline, dashboard and file list. Give it cores and local SSD.
2. **Job workers.** Thumbnails per second follow `FAFNIRX_JOB_WORKERS` (one worker: 18/s on 4 cores). Each worker holds a decoded
   photo, so size it by RAM too.
3. **DB pool.** 2 connections cut timeline reads from 316 to 197 req/s; 32 was no faster than 8 and doubled PostgreSQL's memory.
   About twice the cores is right.
4. **Importing small files** levels off at about 350 files/s from 2 cores up. The cause is not found yet.

`FAFNIRX_WORKER_THREADS` rarely needs changing: one thread served reads as fast as four. Requests queue for the database without
a limit, so with 256 clients at once the slowest 1% waited 2-4 s, though none failed.

**Disk.** Keep PostgreSQL on the machine's own disk. On Docker Desktop, a Mac folder mounted into the container managed 270 small
fsynced writes/s against 821 on the VM's disk, and PostgreSQL reads were 18% slower. Leave `fsync` on: turning it off gained 23%
write throughput, at the cost of safety. SD cards, which are slower at fsync, were not tested.

**Bigger servers.** Not measured; the tests stop at 4 cores. Reads grew about linearly, roughly 80 timeline req/s per core, so
24 cores might reach on the order of 1,500 req/s if PostgreSQL keeps up. Give PostgreSQL most of the cores, set
`FAFNIRX_DB_POOL` to about twice the cores, and raise `FAFNIRX_JOB_WORKERS` (default cap 6) and `FAFNIRX_STREAMS` (default 32) as
RAM and downloads need. Measure with your own library.

## Operations

The image runs as uid 1000 and checks `GET /healthz` every 30 s. `docker compose ps | logs -f master | restart master`;
`docker compose exec db psql -U fafnirx fafnirx`. Watch Dashboard alerts, disk (80%), and each mirror's last success.
