# Installation

Two parts: the **master** on a server, and the **client** on each device. Install the master first: the client needs the
master's address and a token, and the master's web app makes the token.

## 1. The master (Docker)

The master is installed with Docker only, from the image on Docker Hub
([`jameelh99/fafnirx-master`](https://hub.docker.com/r/jameelh99/fafnirx-master), `linux/amd64` and `linux/arm64`).
Compose starts it together with the PostgreSQL it needs. There is nothing to build and no source to clone.

### Requirements

- Docker with the Compose plugin (`docker compose version` works).
- Local disk for PostgreSQL (see [Deployment](deployment.md#storage-layout)). Never put the database on SMB or NFS.
- Disk for the files, at least as large as the data you collect (ZFS recommended).
- Ports `7443/tcp` (devices) and `8080/tcp` (web app). Both are plain HTTP: keep them on your LAN or VPN, or publish
  them through a [Cloudflare tunnel](deployment.md#remote-access-with-a-cloudflare-tunnel).

### Install

```sh
mkdir fafnirx && cd fafnirx
curl -fsSLO https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/docker-compose.yml
cat > .env <<'ENV'
FAFNIRX_ADMIN_PASSWORD=choose-one
FAFNIRX_DB_PASSWORD=another-one
ENV
docker compose up -d
```

Open `http://<server>:8080` and log in as `admin` with `FAFNIRX_ADMIN_PASSWORD`. The password is used once, at the first
start; change it, and add the other users, in the web app (see [Users and roles](usage.md#users-and-roles)).

Check that it runs:

```sh
docker compose ps                    # db is "healthy", master is "Up (healthy)"
docker compose logs master | tail    # ... web app on http://0.0.0.0:8080 ... listening for devices on 0.0.0.0:7443
```

Where things are stored, and other settings, go into `.env` before the first start:

```ini
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # files, versions, updates; share files/ as a network drive
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL files (local disk)
FAFNIRX_CONFIG_DIR=/tank/fafnirx/config  # rclone.conf (rclone remotes for mirrors), kept outside the container
FAFNIRX_UID=1000                         # owner of the files in the data and config dirs
FAFNIRX_GID=1000
FAFNIRX_IMAGE=jameelh99/fafnirx-master:3.0.1   # a version instead of "latest", to upgrade on purpose
```

Create the folders first and give them to `FAFNIRX_UID:FAFNIRX_GID`, or Docker creates them owned by root.

### Upgrade, back up, remove

```sh
docker compose pull && docker compose up -d     # the master migrates its database on start
docker compose down                             # stops it; the data and database folders stay
```

Back up the database before an upgrade: [Deployment](deployment.md#backups). Remote access, a reverse proxy
and the optional video streaming service are in [Deployment](deployment.md) and [Video streaming](streaming.md).

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
| Compose only: `FAFNIRX_IMAGE` | `jameelh99/fafnirx-master:latest` | The image of the master and the streaming service |
| Compose only, with the proxy: `FAFNIRX_WEB_ADDRESS`, `FAFNIRX_DEVICE_ADDRESS`, `FAFNIRX_HTTP_PORT`, `FAFNIRX_HTTPS_PORT`, `FAFNIRX_DEVICE_PORT`, `FAFNIRX_SUBNET` | | See [Reverse proxy](deployment.md#reverse-proxy-with-caddy) |

## 2. A client, on each device

Every platform follows the same three steps: **install**, **enroll**, **add folders**. The client only ever reads the
folders it collects; it never changes, moves or deletes a file in them.

First make the device in the web app: **Devices → Add device**. Give it a name (lowercase letters, digits, `.`, `_`,
`-`); the name is its folder in your folder, `files/<you>/<name>/`, and cannot change. It only has to be unique among your
devices. The next screen shows the **token once**, and the address to use. An administrator can add a device for another
user.

Get the installer from the [latest release](https://github.com/jameelhamdan/fafnirx/releases/latest). Each file has a
`.sha256` next to it.

| System | File |
|---|---|
| Windows | `fafnirx-setup-<version>.exe` |
| macOS, Apple silicon | `fafnirx-<version>-macos-aarch64.tar.gz` |
| macOS, Intel | `fafnirx-<version>-macos-x86_64.tar.gz` |
| Debian, Ubuntu | `fafnirx_<version>-1_amd64.deb`, `fafnirx_<version>-1_arm64.deb` |
| Other Linux | `fafnirx-<version>-linux-x86_64.tar.gz`, `fafnirx-<version>-linux-aarch64.tar.gz` |

### Windows

1. Run `fafnirx-setup-<version>.exe` (if SmartScreen warns, choose *More info → Run anyway*; it does not need administrator
   rights).
2. It asks for the master's address (`http://<server>:7443`), the token and the folder to back up, then enrolls the
   device and starts the client now and at every login.

Silent install, for several machines:

```powershell
fafnirx-setup-<version>.exe /VERYSILENT /SERVER=nas.lan /TOKEN=<token> /FOLDER="C:\Users\me\Documents"
```

### macOS and Linux (one command)

```sh
curl -fsSL https://raw.githubusercontent.com/jameelhamdan/fafnirx/master/installer/get.sh | sh -s -- \
    --server http://<server>:7443 --folder ~/Documents
```

It downloads the release for this machine, checks its `.sha256`, and runs the install below (it asks for the token on
the terminal; `FAFNIRX_TOKEN` and `FAFNIRX_VERSION=<version>` are read from the environment).

### macOS and Linux (tarball)

```sh
tar xzf fafnirx-<version>-<platform>.tar.gz && cd fafnirx-<version>-<platform>
sh install.sh --server http://<server>:7443 --folder ~/Documents      # asks for the token
```

It copies `fafnirx` to `~/.local/bin`, enrolls the device, adds the folders (repeat `--folder` for more) and starts the
client now and at every login: a LaunchAgent on macOS, a systemd user unit on Linux. On Linux, to keep it running without
a login session: `loginctl enable-linger $USER`. If macOS refuses to open the binary because it was downloaded in a
browser, run `xattr -d com.apple.quarantine ~/.local/bin/fafnirx`.

### Debian and Ubuntu (package)

```sh
sudo apt install ./fafnirx_<version>-1_amd64.deb
fafnirx enroll --server http://<server>:7443       # asks for the token; starts the client now and at every login
fafnirx add-folder ~/Documents
```

### Check it

```sh
fafnirx status        # online
fafnirx folders       # what is collected
```

The device shows as online in the web app within a minute and its files appear under **Files → your device**.

### Commands

| Command | |
|---|---|
| `fafnirx enroll --server <url>` | Connects the device (asks for the token) and starts the client in the background and at login. `--no-service` skips that and you run `fafnirx run` yourself. Enrolling again, with a new token or another address, replaces them; a running client switches over within about 20 seconds. |
| `fafnirx add-folder <path>... [--name <name>]` | Collects folders. |
| `fafnirx remove-folder <name>` | Stops collecting one; what is on the master stays there. |
| `fafnirx status`, `folders` | What it is doing; what it collects. |
| `fafnirx pause`, `resume`, `sync` | Stop uploading, continue, run one cycle now. |
| `fafnirx release-deletes` | Send now the deletions held back by the [delete guard](usage.md#when-many-files-disappear). |
| `fafnirx update` | Install a signed update now (the client also checks by itself). |

A folder is stored on the master under its name, as `<you>/<device>/<name>/`. By default the name is the folder's own
name, and a second folder with the same name (`~/photos` and `/mnt/photos`) is named `photos-2`. Choose another with
`--name`. The master does not tell names apart by case or accents (`Photos` and `photos`, or `é` typed in two ways, are
the same), so `add-folder` refuses a name that clashes and suggests a free one. A `.fafnirxignore` file in a folder lists
what to skip (same syntax as `.gitignore`).

The token goes into the operating system's keychain, with a file next to the config as a fallback.

### Which address

- On your own network or over a VPN: `http://<master host>:7443`.
- Over the internet: an `https://` address in front of the master, such as a Cloudflare tunnel. `enroll` warns when you
  give it plain `http://` for a public address, because the token and your files would cross the internet unencrypted.

### Uninstall

Windows: *Apps → fafnirx → Uninstall*. Linux and macOS: `fafnirx uninstall-service`, then delete `~/.local/bin/fafnirx` (or
`sudo apt remove fafnirx`). The files already on the master stay there.

### Build from source

See [Development](development.md#building-the-client-and-the-installers).

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `enroll` prints `warning: could not reach ...` | Wrong address or port, a firewall, or the master is not running. The device is still enrolled; fix the cause and it connects. |
| Client status: `not sending: device in use ...` | Another computer is connected with the same token. Remove it, or use **Release** on the device page (or add a device for this computer). A computer that has been silent for 90 seconds is replaced by itself. |
| Client status: `unknown or revoked device token` | The token was rotated or the device revoked. Enroll again with the new token. |
| `fafnirx status` says it is holding deletions | A large part of a folder disappeared (a drive not mounted?). See [the delete guard](usage.md#when-many-files-disappear). |
| Master exits with `cannot connect to the database` | PostgreSQL is not reachable, or the password differs from the one it was first started with. |
| The web app shows a device offline | No heartbeat for 60 seconds. Check `fafnirx status` on the device. |
| Files copied into `data/files/` take minutes to show up | The mount does not report changes (a Docker Desktop bind mount, another machine writing over NFS). The master compares the whole tree every 5 minutes as a fallback. |
