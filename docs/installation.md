# Installation

Install the **master** on a server first, then the **client** on each device (it needs the master's address and a token made in the web app).

## 1. The master (Docker)

Image: [`jameelh99/fafnirx`](https://hub.docker.com/r/jameelh99/fafnirx) (`amd64`, `arm64`); Compose adds PostgreSQL.

**Requirements:** Docker with Compose; local disk for PostgreSQL (never SMB/NFS; an external server must be PostgreSQL 13+);
disk for the files (ZFS recommended); ports `7443` (devices) and `8080` (web app), both plain HTTP: keep them on a LAN/VPN or
put [TLS in front](deployment.md).

```sh
mkdir fafnirx && cd fafnirx
curl -fsSLO https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/docker/docker-compose.yml
printf 'FAFNIRX_ADMIN_PASSWORD=choose-one\nFAFNIRX_DB_PASSWORD=another-one\nFAFNIRX_PLUGIN_SECRET=%s\n' "$(openssl rand -hex 24)" > .env
docker compose up -d
```

**Hardware.** One core and 512 MB for the whole stack is enough for one person; 2 cores and 1 GB is comfortable. Keep PostgreSQL
on a local SSD. With thumbnails on, `cache/thumbs/` adds 10-20% of the library. Measurements: [Deployment](deployment.md#sizing-and-tuning).

Log in at `http://<server>:8080` as `admin`. The admin password is read once, at the first start. Check: `docker compose ps`.

Optional `.env` (before first start; create the folders and give them to `FAFNIRX_UID:FAFNIRX_GID`):

```ini
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # files, versions; share files/ as a drive
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL, local disk
FAFNIRX_CONFIG_DIR=/tank/fafnirx/config  # rclone.conf
FAFNIRX_UID=1000
FAFNIRX_GID=1000
FAFNIRX_IMAGE=jameelh99/fafnirx:<version>   # pin to upgrade on purpose
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
| `FAFNIRX_PLUGIN_SECRET` | unset | The secret that the optional [modules](modules.md) (add-on containers: [video](video.md), [fetch](sources.md#web-files-and-feeds)) join the master with and are called with, 16+ characters. Without it no add-on may connect |
| `FAFNIRX_PLUGINS` | none | Comma separated ids of the add-ons that must be running (`video,fetch`; the compose file defaults it to `COMPOSE_PROFILES`); one that is not connected or not working shows administrators a warning ([modules](modules.md)) |
| `FAFNIRX_TRUSTED_PROXY` | none | Proxies whose `X-Forwarded-For` is believed ([proxy](deployment.md#reverse-proxy-with-caddy)) |
| `RUST_LOG` | `info` | Log level |
| Tuning: `FAFNIRX_DB_POOL`, `FAFNIRX_JOB_WORKERS`, `FAFNIRX_STREAMS`, `FAFNIRX_TICK_SECS`, `FAFNIRX_INSTANCE_LEASE_SECS` | | Rarely needed: [sizing](deployment.md#sizing-and-tuning) |
| Compose: `FAFNIRX_DB_PASSWORD`, `*_DIR`, `FAFNIRX_UID/GID`, `FAFNIRX_IMAGE`, `FAFNIRX_VIDEO_IMAGE`, `FAFNIRX_FETCH_IMAGE`; with proxy `FAFNIRX_WEB_ADDRESS`, `FAFNIRX_DEVICE_ADDRESS`, `FAFNIRX_HTTP_PORT`, `FAFNIRX_HTTPS_PORT`, `FAFNIRX_DEVICE_PORT`, `FAFNIRX_SUBNET` | | Compose only |

## 2. A client

**Install → enroll → add folders**, in one command, `fafnirx install`. The client only reads the folders it collects.

First, in the web app: **Devices → Add device**. The name (`a-z 0-9 . _ -`) becomes the folder `files/users/<you>/<name>/` and cannot
change; the token is shown **once**. Administrators can add devices for other users.

Then, on the device (each downloads the program from the [latest release](https://github.com/jameelhamdan/fafnirx/releases/latest),
checks its `.sha256` and runs `fafnirx install`, which asks for the token, or reads `FAFNIRX_TOKEN`; `FAFNIRX_VERSION=<version>` picks another release than the latest):

| System | Command |
|---|---|
| Linux, macOS | `curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.sh \| sh -s -- --server nas.lan --folder ~/Documents` |
| Windows (PowerShell) | `irm https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.ps1 -OutFile get.ps1; .\get.ps1 -Server nas.lan -Folder $HOME\Documents` |
| Windows (setup) | Run `fafnirx-setup-<version>.exe` from the release (SmartScreen: *More info* → *Run anyway*; no administrator rights). It asks for the server, the token and a folder. Silent: `fafnirx-setup-<version>.exe /VERYSILENT /SERVER=nas.lan /TOKEN=<token> [/FOLDER="C:\Users\me\Documents"]` |
| Debian, Ubuntu | `sudo apt install ./fafnirx_<version>-1_amd64.deb` (or `arm64`), then `sudo fafnirx install --server nas.lan --folder ~/Documents` |
| macOS (Homebrew) | `brew install jameelhamdan/fafnirx/fafnirx`, then `sudo fafnirx install --server nas.lan --folder ~/Documents` |
| A downloaded program | `sudo ./fafnirx-<version>-<os>-<arch> install --server nas.lan` (Windows: `.\fafnirx-<version>-windows-x86_64.exe install --server nas.lan`) |

`--server` takes `nas.lan`, `nas.lan:7443` or a full URL (`https://...` behind a TLS proxy); without a scheme it is plain `http` on port
`7443`. `--folder` can be repeated. `fafnirx install --dry-run` says what it would do and changes nothing.

**What `fafnirx install` does**, in this order, and the same everywhere: copies the program to `/usr/local/bin/fafnirx` (Linux, macOS; owned
by root, because the service runs as root; `FAFNIRX_ALLOW_UNPROTECTED_EXE=1` lifts the check for a build on the machine it was made on) or `%LocalAppData%\Programs\fafnirx` (Windows; and adds it to your `PATH`, unless
`--no-path`); enrolls this device with the master; collects the folders; installs the service and the tray (below) and starts them;
removes older copies of `fafnirx` that other installs left. Run again on an enrolled device it brings it up to date. With
`--in-place` it does not copy the program (a package or the setup put it there); with `--no-start` it installs no service. macOS
blocking the program: `sudo xattr -d com.apple.quarantine /usr/local/bin/fafnirx`.

**macOS Full Disk Access.** To sync folders in Documents, Desktop, Downloads, iCloud Drive or on another volume, give the client
Full Disk Access: *System Settings* → *Privacy & Security* → *Full Disk Access* → `+` → `/usr/local/bin/fafnirx`, switch it on, then
`sudo fafnirx restart`. The install says so at the end; without it these folders fail with `Operation not permitted` (see
Troubleshooting).

On Linux and macOS the client is **a service of the device**: it starts at boot, is started again when it fails, reads the
folders of every user, and its tray starts when a user logs in. Its config, token and state are in `/etc/fafnirx` and
`/var/lib/fafnirx` (Linux), `/Library/Application Support/fafnirx` (macOS), where only administrators can change them; commands
that change them ask for administrator rights (`sudo`); the tray's folder entries do not, the service applies them. On Windows the client is **yours**: it and its tray start when you log
in, and its config (`%LocalAppData%\fafnirx\config.toml`), token (Credential Manager) and state are in your profile, so the tray
and the commands change them without asking for anything.

Check with `fafnirx status` and `fafnirx folder list`; the device shows online within a minute. [Commands](usage.md#the-client).

**Other clients:** for Android, use [fafnirx-android](https://github.com/jameelhamdan/fafnirx-android).

A folder is stored as `<you>/<device>/<name>/`; the name defaults to the folder's own (a second `photos` becomes `photos-2`) or
use `--name`. Names are compared ignoring case and accents; `folder add` refuses a clash. **Address:** on a LAN/VPN
`http://<master>:7443`; over the internet an `https://` address in front of the master (`install` warns about plain `http://`).

**Uninstall:** `sudo fafnirx uninstall` (Windows: `fafnirx uninstall`, or *Apps* → *fafnirx*; Debian: `sudo apt remove fafnirx`) stops the
client and the tray and removes the program; `--purge` also removes the config, the token and the state of this client. Data on the master
stays. **From source:** [Development](development.md#build-and-run).

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `install`: `could not reach ...` | Wrong address/port, firewall, master down. The device is enrolled and connects once fixed |
| `not sending: device in use ...` | Another device uses the token: **Release** on the device page, or wait 90 s |
| `unknown or revoked device token` | Token rotated or device revoked: run `fafnirx install --server <master>` again with the new token |
| `status` says holding deletions | A large part of a folder vanished: [delete guard](usage.md#the-delete-guard) |
| `folder add`: `... overlaps ... collect one of them` | A folder cannot sit inside, or contain, another collected folder. To share a folder inside one you collect, `folder remove` the outer one and `folder add` its other subfolders (the master keeps its data) |
| macOS: `cannot replace the file: Operation not permitted` (or the tray says `Needs Full Disk Access`) | The client lacks Full Disk Access for a folder in Documents, Desktop, Downloads, iCloud Drive or another volume: *Privacy & Security* → *Full Disk Access* → add `/usr/local/bin/fafnirx`, then `sudo fafnirx restart`. If it is not that: the file is locked (`ls -lO` shows `uchg`; `sudo chflags nouchg <file>`) |
| Master: `cannot connect to the database` | PostgreSQL unreachable, or password differs from the first start's |
| Device shown offline | No heartbeat for 60 s: check `fafnirx status` |
| Files copied into `data/files/` take minutes | Mount reports no changes; the master compares the tree every 5 minutes |
