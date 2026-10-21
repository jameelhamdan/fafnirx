# Installation

Two parts: the **orchestrator** on a Linux server (Docker), and the **client** on each device.

## 1. The orchestrator

### Requirements

- A Linux host with Docker and the Compose plugin.
- Local disk for PostgreSQL (see [Deployment](deployment.md#storage-layout)). Never put the database on SMB or NFS.
- Disk for the chunk store, at least as large as the data you collect (ZFS recommended).
- Ports `7443/tcp` (devices, TLS) and `8080/tcp` (web UI, plain HTTP, LAN or VPN only).

### Steps

```sh
git clone <this repository> fafnirx && cd fafnirx

export FAFNIRX_ADMIN_PASSWORD='choose-one'       # web UI login
export FAFNIRX_DB_PASSWORD='another-one'         # PostgreSQL; letters and digits keep the URL simple
export FAFNIRX_TLS_HOSTS='nas.lan,192.168.1.5'   # every name/IP your devices will use to reach the server
docker compose up -d --build
```

To keep settings between shells, put them in a `.env` file next to `docker-compose.yml`:

```ini
FAFNIRX_ADMIN_PASSWORD=choose-one
FAFNIRX_DB_PASSWORD=another-one
FAFNIRX_TLS_HOSTS=nas.lan,192.168.1.5
FAFNIRX_DATA_DIR=/tank/fafnirx/data      # chunk store, certificates, updates
FAFNIRX_DB_DIR=/tank/fafnirx/pgdata      # PostgreSQL files
FAFNIRX_UID=1000                         # owner of the files in the data dir
FAFNIRX_GID=1000
```

Compose starts three services: `db` (PostgreSQL 17), `engine` (`fafnirx-server`) and `web`. The schema is created on
first start. Open `http://<host>:8080` and log in with the admin password.

### Check that it works

```sh
docker compose ps                  # db is "healthy", engine and web are "Up"
docker compose logs engine | tail  # ... fafnirx-server listening on 0.0.0.0:7443 (TLS, ...)
curl -I http://localhost:8080/login
```

### Configuration reference

| Variable | Used by | Default | Meaning |
|---|---|---|---|
| `FAFNIRX_ADMIN_PASSWORD` | web | required | Web UI password |
| `FAFNIRX_DB_PASSWORD` | compose | required | PostgreSQL password (compose builds `FAFNIRX_DATABASE_URL` from it) |
| `FAFNIRX_DATABASE_URL` | engine, web | required | `postgresql://user:password@host/database` |
| `FAFNIRX_TLS_HOSTS` | engine | `localhost,127.0.0.1` | Names and IPs for the server certificate |
| `FAFNIRX_PUBLIC_ADDR` | web | `https://<request host>:7443` | Server address shown in device setup instructions |
| `FAFNIRX_DATA_DIR` | compose | `./data` | Host path of the data directory |
| `FAFNIRX_DB_DIR` | compose | `./pgdata` | Host path of PostgreSQL's files |
| `FAFNIRX_UID`, `FAFNIRX_GID` | compose | `1000` | User the containers run as |
| `FAFNIRX_DATA` | engine, web | `data` (`/data` in Docker) | Data directory inside the container |
| `FAFNIRX_LISTEN` | engine | `0.0.0.0:7443` | gRPC listen address |
| `FAFNIRX_WEB_HOST`, `FAFNIRX_WEB_PORT` | web | `0.0.0.0`, `8080` | Web UI listen address |
| `FAFNIRX_RCLONE` | engine | `rclone` | rclone binary used by mirrors |
| `FAFNIRX_TICK_SECS` | engine | `15` | How often due sources and mirrors are looked for |
| `RUST_LOG` | engine, client | `info` | Log level |

### Using your own PostgreSQL server

Remove the `db` service from `docker-compose.yml`, create an empty database and a user that owns it, and set
`FAFNIRX_DATABASE_URL` on `engine` and `web`. PostgreSQL 17 is what is tested; the SQL used (identity columns, `ON CONFLICT`, `SKIP LOCKED`) needs 10 or newer. The connection is
not encrypted, so keep it on a trusted network.

## 2. The client

### Get the binary

Build it (needs the [Rust toolchain](https://rustup.rs)):

```sh
cargo build --release -p fafnirx          # target/release/fafnirx
```

On Linux the tray icon needs GTK and appindicator development packages. Leave it out with
`cargo build --release -p fafnirx --no-default-features`.

Copy the binary to the device. Later versions are delivered by [signed updates](usage.md#signed-client-updates).

### Enroll the device

1. In the web UI, add a device. Its name is its namespace on the server (`devices/<name>/`). Copy the token; it is
   shown once.
2. Download the CA certificate (**CA certificate** in the top bar, or `http://<host>:8080/ca.pem`).
3. On the device:

```sh
fafnirx enroll --server https://nas.lan:7443 --token <token> --ca fafnirx-ca.pem
fafnirx add-folder ~/Documents --name documents
fafnirx run                    # try it in the foreground first
fafnirx install-service        # then start automatically at login
```

`install-service` registers a systemd user unit (Linux), a LaunchAgent (macOS) or a per-user Run key (Windows, no
administrator rights needed). The server address must match one of `FAFNIRX_TLS_HOSTS`, or TLS verification fails.

The token goes into the OS keychain, with a file fallback. A reinstalled client reuses the same device token and
namespace.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `enroll` fails with a certificate error | The address is not in `FAFNIRX_TLS_HOSTS`. Add it and `docker compose up -d`; the server certificate is reissued and the CA stays the same, so enrolled devices keep working. |
| `UnknownIssuer` on Windows | Antivirus HTTPS scanning replaces the certificate, even on loopback. Exclude fafnirx from it. |
| Engine exits with `cannot connect to the database` | PostgreSQL is not reachable or the password differs from the one it was first started with. |
| Web UI shows a device offline | No heartbeat for 60 seconds. Check `fafnirx status` on the device. |
