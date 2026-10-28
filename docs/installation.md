# Installation

Install the **master** on a server first, then the **client** on each device (it needs the master's address and a token made in the web app).

## 1. The master (Docker)

Image: [`jameelh99/fafnirx-master`](https://hub.docker.com/r/jameelh99/fafnirx-master) (`amd64`, `arm64`); Compose adds PostgreSQL.

**Requirements:** Docker with Compose; local disk for PostgreSQL (never SMB/NFS; an external server must be PostgreSQL 13+);
disk for the files (ZFS recommended); ports `7443` (devices) and `8080` (web app), both plain HTTP: keep them on a LAN/VPN or
put [TLS in front](deployment.md).

```sh
mkdir fafnirx && cd fafnirx
curl -fsSLO https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/docker-compose.yml
printf 'FAFNIRX_ADMIN_PASSWORD=choose-one\nFAFNIRX_DB_PASSWORD=another-one\n' > .env
docker compose up -d
```

Log in at `http://<server>:8080` as `admin`. The admin password is read once, at the first start. Check: `docker compose ps`.

Optional `.env` (before first start; create the folders and give them to `FAFNIRX_UID:FAFNIRX_GID`):

```ini
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # files, versions; share files/ as a drive
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL, local disk
FAFNIRX_CONFIG_DIR=/tank/fafnirx/config  # rclone.conf
FAFNIRX_UID=1000
FAFNIRX_GID=1000
FAFNIRX_IMAGE=jameelh99/fafnirx-master:3.0.1   # pin to upgrade on purpose
```

**Upgrade:** `docker compose pull && docker compose up -d` (migrates on start; [back up first](deployment.md#backups)).
**Stop:** `docker compose down` (data stays).

| Variable | Default | Meaning |
|---|---|---|
| `FAFNIRX_DATABASE_URL` | required | `postgresql://user:password@host/database` |
| `FAFNIRX_ADMIN_PASSWORD` | required until set | Password of `admin`, first start only |
| `FAFNIRX_DATA` | `data` (`/data`) | Data directory |
| `FAFNIRX_LISTEN`, `FAFNIRX_HTTP` | `0.0.0.0:7443`, `0.0.0.0:8080` | Device (gRPC) and web addresses |
| `FAFNIRX_RCLONE`, `FAFNIRX_RCLONE_CONFIG` | `rclone`, `<data>/rclone.conf` (`/config/rclone.conf`) | rclone binary and config for mirrors |
| `FAFNIRX_STREAM_URL`, `FAFNIRX_STREAM_TOKEN` | unset | Streaming service and secret ([streaming](streaming.md)) |
| `FAFNIRX_FFMPEG`, `FAFNIRX_FFPROBE` | `ffmpeg`, `ffprobe` | Video details, pictures, playable copies |
| `FAFNIRX_TICK_SECS` | `15` | How often due sources and mirrors are looked for |
| `FAFNIRX_TRUSTED_PROXY` | none | Proxies whose `X-Forwarded-For` is believed ([proxy](deployment.md#reverse-proxy-with-caddy)) |
| `FAFNIRX_INSTANCE_LEASE_SECS` | `90` | Silence before another computer may take over a device |
| `FAFNIRX_MAP_TILES` | OpenStreetMap | Tile URL with `{z}/{x}/{y}`; set your own offline |
| `RUST_LOG` | `info` | Log level |
| Compose: `FAFNIRX_DB_PASSWORD`, `*_DIR`, `FAFNIRX_UID/GID`, `FAFNIRX_IMAGE`; with proxy `FAFNIRX_WEB_ADDRESS`, `FAFNIRX_DEVICE_ADDRESS`, `FAFNIRX_HTTP_PORT`, `FAFNIRX_HTTPS_PORT`, `FAFNIRX_DEVICE_PORT`, `FAFNIRX_SUBNET` | | Compose only |

## 2. A client

**Install → enroll → add folders.** The client only reads the folders it collects.

First, in the web app: **Devices → Add device**. The name (`a-z 0-9 . _ -`) becomes the folder `files/users/<you>/<name>/` and cannot
change; the token is shown **once**. Administrators can add devices for other users.

Download from the [latest release](https://github.com/jameelhamdan/fafnirx/releases/latest) (each file has a `.sha256`):
Windows `fafnirx-setup-<version>.exe`; macOS `fafnirx-<version>-macos-{aarch64,x86_64}.tar.gz`; Debian/Ubuntu
`fafnirx_<version>-1_{amd64,arm64}.deb`; other Linux `fafnirx-<version>-linux-{x86_64,aarch64}.tar.gz`.

- **Windows:** run the setup (SmartScreen: *More info → Run anyway*; no admin rights). It asks for `http://<server>:7443`, the
  token and a folder, then enrolls and starts the client now and at login. Silent:
  `fafnirx-setup-<version>.exe /VERYSILENT /SERVER=nas.lan /TOKEN=<token> /FOLDER="C:\Users\me\Documents"`.
- **macOS, Linux:**
  ```sh
  curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.sh | sh -s -- \
      --server http://<server>:7443 --folder ~/Documents
  ```
  It downloads the release, checks the `.sha256`, installs to `~/.local/bin`, enrolls (asks for the token; or
  `FAFNIRX_TOKEN`, `FAFNIRX_VERSION`), adds folders (repeat `--folder`) and starts at login (LaunchAgent / systemd user unit).
  From a tarball run `sh install.sh ...`. Linux without a login session: `loginctl enable-linger $USER`. macOS blocking the
  binary: `xattr -d com.apple.quarantine ~/.local/bin/fafnirx`.
- **Debian, Ubuntu:** `sudo apt install ./fafnirx_<version>-1_amd64.deb`, then `fafnirx enroll --server http://<server>:7443` and `fafnirx add-folder ~/Documents`.

Check with `fafnirx status` and `fafnirx folders`; the device shows online within a minute. [Commands](usage.md#the-client).

**Other clients:** for Android, use [fafnirx-android](https://github.com/jameelhamdan/fafnirx-android).

A folder is stored as `<you>/<device>/<name>/`; the name defaults to the folder's own (a second `photos` becomes `photos-2`) or
use `--name`. Names are compared ignoring case and accents; `add-folder` refuses a clash. **Address:** on a LAN/VPN
`http://<master>:7443`; over the internet an `https://` address in front of the master (`enroll` warns about plain `http://`).

**Uninstall:** Windows *Apps → fafnirx*; Linux/macOS `fafnirx uninstall-service` and delete `~/.local/bin/fafnirx` (or
`sudo apt remove fafnirx`). Data on the master stays. **From source:** [Development](development.md#build-and-run).

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `enroll`: `could not reach ...` | Wrong address/port, firewall, master down. The device is enrolled and connects once fixed |
| `not sending: device in use ...` | Another computer uses the token: **Release** on the device page, or wait 90 s |
| `unknown or revoked device token` | Token rotated or device revoked: enroll again |
| `status` says holding deletions | A large part of a folder vanished: [delete guard](usage.md#the-delete-guard) |
| Master: `cannot connect to the database` | PostgreSQL unreachable, or password differs from the first start's |
| Device shown offline | No heartbeat for 60 s: check `fafnirx status` |
| Files copied into `data/files/` take minutes | Mount reports no changes; the master compares the tree every 5 minutes |
