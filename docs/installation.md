# Installation

Two parts: the **master** on a server, and the **client** on each device. A client needs the master's address and a
token; the master's web app makes both.

## 1. The master

### Requirements

- Docker with the Compose plugin. The master is supported as a Docker image only; Compose also starts the PostgreSQL it needs.
- Local disk for PostgreSQL (see [Deployment](deployment.md#storage-layout)). Never put the database on SMB or NFS.
- Disk for the files, at least as large as the data you collect (ZFS recommended).
- Ports `7443/tcp` (devices) and `8080/tcp` (web app). Both are plain HTTP: keep them on your LAN or VPN, or publish
  them through a [Cloudflare tunnel](deployment.md#remote-access-with-a-cloudflare-tunnel).

### Install

```sh
git clone https://github.com/jameelhamdan/fafnirx.git fafnirx && cd fafnirx
cat > .env <<'ENV'
FAFNIRX_ADMIN_PASSWORD=choose-one
FAFNIRX_DB_PASSWORD=another-one
ENV
docker compose up -d --build
```

Open `http://<server>:8080` and log in as `admin` with `FAFNIRX_ADMIN_PASSWORD`. Add the other users on the **Users** page (see
[Users and roles](usage.md#users-and-roles)). More settings for `.env`:

```ini
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # files, versions, updates; share files/ as a network drive
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL files (local disk)
FAFNIRX_CONFIG_DIR=/tank/fafnirx/config  # rclone.conf (rclone remotes for mirrors), kept outside the container
FAFNIRX_UID=1000                         # owner of the files in the data and config dirs
FAFNIRX_GID=1000
```

Compose starts `db` (PostgreSQL 17) and `master`. The schema is created on first start.

```sh
docker compose ps                    # db is "healthy", master is "Up"
docker compose logs master | tail    # ... web app on http://0.0.0.0:8080 ... listening for devices on 0.0.0.0:7443
```

### Configuration reference

| Variable | Default | Meaning |
|---|---|---|
| `FAFNIRX_DATABASE_URL` | required | `postgresql://user:password@host/database` |
| `FAFNIRX_ADMIN_PASSWORD` | required until set | Password of the user `admin`, set at the first start; after that it is changed in the web app and this is not read |
| `FAFNIRX_DATA` | `data` (`/data` in Docker) | Data directory |
| `FAFNIRX_LISTEN` | `0.0.0.0:7443` | Device (gRPC) listen address |
| `FAFNIRX_HTTP` | `0.0.0.0:8080` | Web app and API listen address |
| `FAFNIRX_RCLONE` | `rclone` | rclone binary used by mirrors |
| `FAFNIRX_STREAM_URL`, `FAFNIRX_STREAM_TOKEN` | unset | Where the optional streaming service is and the secret shared with it; see [Video streaming](streaming.md) |
| `FAFNIRX_RCLONE_CONFIG` | `<FAFNIRX_DATA>/rclone.conf` (`/config/rclone.conf` in Docker) | rclone config file for mirrors that name a `remote` |
| `FAFNIRX_TICK_SECS` | `15` | How often due sources and mirrors are looked for |
| `FAFNIRX_TRUSTED_PROXY` | none | Addresses or ranges (comma separated) of reverse proxies in front of the master. From these, `X-Forwarded-For` is believed about the client's address, so logins are counted per person and not per proxy. [Reverse proxy](deployment.md#reverse-proxy-with-caddy) |
| `FAFNIRX_INSTANCE_LEASE_SECS` | `90` | How long a device must be silent before another computer may take over its token (see [Usage](usage.md)) |
| `FAFNIRX_FFMPEG`, `FAFNIRX_FFPROBE` | `ffmpeg`, `ffprobe` | Programs that read videos and make their pictures and playable copies |
| `FAFNIRX_MAP_TILES` | `https://tile.openstreetmap.org/{z}/{x}/{y}.png` | Where the browser loads the map's pictures from; point it at a tile server of your own on a network without internet |
| `RUST_LOG` | `info` | Log level |
| Compose only: `FAFNIRX_DB_PASSWORD`, `FAFNIRX_DATA_DIR`, `FAFNIRX_DB_DIR`, `FAFNIRX_CONFIG_DIR`, `FAFNIRX_UID`, `FAFNIRX_GID` | | |
| Compose only: `FAFNIRX_IMAGE` | `fafnirx-master:local` | The image of the master and the streaming service. Built from this folder by default; on a server set it to the release on Docker Hub (`<user>/fafnirx-master:<version>`) and `docker compose pull && docker compose up -d` |
| Compose only, with the proxy: `FAFNIRX_WEB_ADDRESS`, `FAFNIRX_DEVICE_ADDRESS`, `FAFNIRX_HTTP_PORT`, `FAFNIRX_HTTPS_PORT`, `FAFNIRX_DEVICE_PORT`, `FAFNIRX_SUBNET` | | See [Reverse proxy](deployment.md#reverse-proxy-with-caddy) |

## 2. Adding a device

In the web app: **Devices → Add device**. Give it a name (lowercase letters, digits, `.`, `_`, `-`); the name is its
folder in your folder, `files/<you>/<name>/`, and cannot change. The name only has to be unique among your devices, and a device
always belongs to a user (an administrator can add one for another user). The next screen shows the token **once**, with the command to run.

### Windows

Download `fafnirx-setup-<version>.exe` from the [release](#from-a-release), or build it yourself (needs [Inno Setup](https://jrsoftware.org/isinfo.php)):

```powershell
installer\build.ps1                 # writes dist\fafnirx-setup-<version>.exe
```

Run the setup: it asks for the master's address, the token and the folder to back up, enrolls the device, and starts
the client now and at every login (no administrator rights). Silent install:

```powershell
fafnirx-setup-0.2.0.exe /VERYSILENT /SERVER=nas.lan /TOKEN=<token> /FOLDER="C:\Users\me\Documents"
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

A folder is stored on the master under its name, as `<you>/<device>/<name>/`. By default the name is the folder's own name, and
a second folder with the same name (`~/photos` and `/mnt/photos`) is named `photos-2`. Choose another with `--name`. The
master does not tell names apart by case or accents (`Photos` and `photos`, or `é` typed in two ways, are the same), so
`add-folder` refuses a name that clashes and suggests a free one.

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
| Client status: `not sending: device in use ...` | Another computer is connected with the same token. Remove it, or use **Release** on the device page (or add a device for this computer). A computer that has been silent for 90 seconds is replaced by itself. |
| Client status: `unknown or revoked device token` | The token was rotated or the device revoked. Enroll again with the new token. |
| Master exits with `cannot connect to the database` | PostgreSQL is not reachable, or the password differs from the one it was first started with. |
| The web app shows a device offline | No heartbeat for 60 seconds. Check `fafnirx status` on the device. |
| Files copied into `data/files/` take minutes to show up | The mount does not report changes (a Docker Desktop bind mount, another machine writing over NFS). The master compares the whole tree every 5 minutes as a fallback. |
