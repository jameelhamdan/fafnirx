# Installation

Two parts: the **master** on a server, and the **client** on each device. A client needs the master's address and a
token; the master's web app makes both.

## 1. The master

### Requirements

- Docker with the Compose plugin (the easy way), or Linux with PostgreSQL 14 or newer and the `fafnirx-master` binary.
- Local disk for PostgreSQL (see [Deployment](deployment.md#storage-layout)). Never put the database on SMB or NFS.
- Disk for the files, at least as large as the data you collect (ZFS recommended).
- Ports `7443/tcp` (devices) and `8080/tcp` (web app). Both are plain HTTP: keep them on your LAN or VPN, or publish
  them through a [Cloudflare tunnel](deployment.md#remote-access-with-a-cloudflare-tunnel).

### With Docker

```sh
git clone https://github.com/jameelhamdan/fafnirx.git fafnirx && cd fafnirx
cat > .env <<'ENV'
FAFNIRX_ADMIN_PASSWORD=choose-one
FAFNIRX_DB_PASSWORD=another-one
ENV
docker compose up -d --build
```

Open `http://<server>:8080` and log in. More settings for `.env`:

```ini
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # files, versions, updates; share files/ as a network drive
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL files (local disk)
FAFNIRX_UID=1000                         # owner of the files in the data dir
FAFNIRX_GID=1000
```

Compose starts `db` (PostgreSQL 17) and `master`. The schema is created on first start.

```sh
docker compose ps                    # db is "healthy", master is "Up"
docker compose logs master | tail    # ... admin web app on http://0.0.0.0:8080 ... listening for devices on 0.0.0.0:7443
```

### Without Docker (Linux)

Build with the web app first, because it is embedded in the binary:

```sh
(cd frontend && npm ci && npm run build)
cargo build --release -p fafnirx-master -p fafnirx-sign
sudo install target/release/fafnirx-master /usr/local/bin/
```

Create a database and user in PostgreSQL, write `/etc/fafnirx/master.env` as described in
[`installer/fafnirx-master.service`](../installer/fafnirx-master.service), and enable that unit. Install `rclone` if you
want mirrors, and `ffmpeg` (which brings `ffprobe`) if you want video posters, details and playable copies in the photo
library. Photos need nothing extra.

### Configuration reference

| Variable | Default | Meaning |
|---|---|---|
| `FAFNIRX_DATABASE_URL` | required | `postgresql://user:password@host/database` |
| `FAFNIRX_ADMIN_PASSWORD` | required | Password of the web app |
| `FAFNIRX_DATA` | `data` (`/data` in Docker) | Data directory |
| `FAFNIRX_ROLE` | `all` | `all`, or a comma list of `api`, `transfer`, `worker` ([roles](deployment.md#splitting-the-roles)) |
| `FAFNIRX_LISTEN` | `0.0.0.0:7443` | Device (gRPC) listen address |
| `FAFNIRX_HTTP` | `0.0.0.0:8080` | Web app and admin API listen address |
| `FAFNIRX_RCLONE` | `rclone` | rclone binary used by mirrors |
| `FAFNIRX_TICK_SECS` | `15` | How often due sources and mirrors are looked for |
| `FAFNIRX_FFMPEG`, `FAFNIRX_FFPROBE` | `ffmpeg`, `ffprobe` | Programs that read videos and make their pictures and playable copies |
| `FAFNIRX_MAP_TILES` | `https://tile.openstreetmap.org/{z}/{x}/{y}.png` | Where the browser loads the map's pictures from; point it at a tile server of your own on a network without internet |
| `RUST_LOG` | `info` | Log level |
| Compose only: `FAFNIRX_DB_PASSWORD`, `FAFNIRX_DATA_DIR`, `FAFNIRX_DB_DIR`, `FAFNIRX_UID`, `FAFNIRX_GID` | | |

## 2. Adding a device

In the web app: **Devices → Add device**. Give it a name (lowercase letters, digits, `.`, `_`, `-`); the name is its
folder, `files/<name>/`, and cannot change. The next screen shows the token **once**, with the command to run.

### Windows

Download `fafnirx-setup-<version>.exe` from the [release](#from-a-release), or build it yourself (needs [Inno Setup](https://jrsoftware.org/isinfo.php)):

```powershell
installer\build.ps1                 # writes dist\fafnirx-setup-<version>.exe
```

Run the setup: it asks for the master's address, the token and the folder to back up, enrolls the device, and starts
the client now and at every login (no administrator rights). Silent install:

```powershell
fafnirx-setup-0.1.4.exe /VERYSILENT /SERVER=nas.lan /TOKEN=<token> /FOLDER="C:\Users\me\Documents"
```

### Linux and macOS

```sh
cargo build --release -p fafnirx                      # target/release/fafnirx
sh installer/install.sh --server http://nas.lan:7443 --folder ~/Documents      # asks for the token
```

The script copies the binary to `~/.local/bin`, enrolls the device, adds the folders and starts the client (a systemd user
unit on Linux, a LaunchAgent on macOS). On Linux the tray icon needs GTK and appindicator development packages; build
with `--no-default-features` to leave it out. To keep the client running without an open login session on Linux:
`loginctl enable-linger $USER`.

### From a release

Each [GitHub release](https://github.com/jameelhamdan/fafnirx/releases) carries the client built for every platform,
with a `.sha256` file for each:

| File | For |
|---|---|
| `fafnirx-setup-<version>.exe` | Windows (the setup above) |
| `fafnirx-<version>-macos-aarch64.tar.gz`, `-macos-x86_64.tar.gz` | macOS, Apple silicon and Intel |
| `fafnirx-<version>-linux-x86_64.tar.gz`, `-linux-aarch64.tar.gz` | Linux; contains `install.sh` |
| `fafnirx_<version>_amd64.deb`, `_arm64.deb` | Debian and Ubuntu: `sudo apt install ./fafnirx_<version>_amd64.deb`, then enroll |

For a tarball: `tar xzf fafnirx-<version>-<platform>.tar.gz && cd fafnirx-<version>-<platform> && sh install.sh --server ... --folder ...`.

### By hand, on any platform

```sh
fafnirx enroll --server http://nas.lan:7443     # asks for the token; also starts it in the background and at login
fafnirx add-folder ~/Documents
fafnirx status                                                   # online
```

Pass `--no-service` to `enroll` to skip the background service and run `fafnirx run` yourself.

The token goes into the operating system's keychain, with a file next to the config as a fallback. Enrolling again
(with a new token after a rotation, or another server address) replaces it, and a running client switches over by
itself within about 20 seconds.

### Which address

- On your own network or over a VPN: `http://<master host>:7443`.
- Over the internet: an `https://` address in front of the master, such as a Cloudflare tunnel. `enroll` warns when you
  give it plain `http://` for a public address, because the token and your files would cross the internet unencrypted.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `enroll` prints `warning: could not reach ...` | Wrong address or port, a firewall, or the master is not running. The device is still enrolled; fix the cause and it connects. |
| Client status: `unknown or revoked device token` | The token was rotated or the device revoked. Enroll again with the new token. |
| Master exits with `cannot connect to the database` | PostgreSQL is not reachable, or the password differs from the one it was first started with. |
| The web app shows a device offline | No heartbeat for 60 seconds. Check `fafnirx status` on the device. |
| Files copied into `data/files/` take minutes to show up | The mount does not report changes (a Docker Desktop bind mount, another machine writing over NFS). The master compares the whole tree every 5 minutes as a fallback. |
