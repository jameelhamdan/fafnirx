# Architecture

## Components

```mermaid
flowchart LR
    subgraph Devices
        C1["fafnirx client<br/>Windows"]
        C2["fafnirx client<br/>macOS"]
        C3["fafnirx client<br/>Linux"]
    end

    subgraph Orchestrator["Orchestrator (Docker Compose, Linux)"]
        ENG["engine: fafnirx-server<br/>gRPC :7443 (TLS)<br/>Rust"]
        WEB["web: FastAPI :8080<br/>Python"]
        PG[("db: PostgreSQL<br/>metadata")]
        FS[("data dir<br/>chunk store, certs, updates")]
        ENG --> PG
        ENG --> FS
        WEB --> PG
        WEB -. "reads certs" .-> FS
    end

    GD["Google Drive"]
    subgraph Mirrors
        RC["rclone targets<br/>S3, SFTP, WebDAV, ..."]
    end

    C1 & C2 & C3 -- "upload only" --> ENG
    GD -- "scheduled import" --> ENG
    ENG -- "scheduled mirror (rclone)" --> RC
    ADMIN(["Admin browser"]) --> WEB
```

| Component | Language | Job |
|---|---|---|
| Client `fafnirx` (`crates/client`) | Rust | Watch folders, chunk and hash, upload, queue offline, tray icon, self-update |
| Engine `fafnirx-server` (`crates/server`) | Rust | gRPC data path, chunk store, ingest, retention and garbage collection, alerts, Drive import, mirror runs |
| Web UI (`web/fafnirx_web`) | Python | Admin UI: devices and tokens, sources, mirrors, settings, alerts |
| `fafnirx-sign` (`crates/sign`) | Rust | Create the update key, publish signed client builds |
| Common (`crates/common`) | Rust | Generated gRPC types, chunking, hashing and path rules shared by client and engine |
| PostgreSQL | - | All metadata. The only database; both processes share it |

The engine and the web UI never call each other. They coordinate **only through the database**: the web UI sets
`run_requested = 1` on a source or mirror and the engine's scheduler picks it up on its next tick; the engine writes
`last_status`, alerts and heartbeats that the web UI displays.

Design decisions worth knowing:

- **One-way by construction.** The gRPC service has no call that reads, lists or deletes stored data. `Negotiate` only
  tells a client which of its own chunk hashes are missing.
- **Content-addressed storage.** A chunk is a file named by its BLAKE3 hash, so duplicates collapse and uploads are
  idempotent.
- **Metadata is separate from data.** Losing PostgreSQL loses the map from paths to chunks, not the chunks; losing a
  chunk loses data. Back up both (see [Deployment](deployment.md#backups)).
- **Single orchestrator** on purpose; offsite mirrors are the backup.

## Upload path

```mermaid
sequenceDiagram
    participant C as Client
    participant S as Engine (gRPC)
    participant FS as Chunk store
    participant DB as PostgreSQL

    C->>S: Heartbeat (hostname, OS, disk, recent errors)
    S->>DB: update device, open or resolve alerts

    C->>S: Negotiate(manifest)
    S->>DB: current version of this path has this root hash?
    S->>FS: which chunks exist?
    S-->>C: missing chunk indexes (or already_current)

    C->>S: UploadChunks (stream of chunks)
    S->>S: hash each chunk, reject mismatches
    S->>FS: write to tmp/, fsync, atomic rename into chunks/ab/cd/hash

    C->>S: Commit(manifest, mtime, mode, renamed_from?)
    S->>FS: every chunk present?
    S->>DB: one transaction: file row, new version, prune old versions, event, device.last_seen
    S-->>C: version id
```

Every RPC carries the device token (`authorization: Bearer ...`), is checked against the SHA-256 hash stored in
`devices.token_hash`, and only travels over TLS. The server certificate is signed by a CA the engine creates on first
start; clients pin that CA.

The commit is a single database transaction. If the file already has a version with the same root hash, the commit is
a no-op that returns the existing version.

## Background work in the engine

```mermaid
flowchart TB
    subgraph Every_minute["Every minute"]
        A1["Offline devices:<br/>open or resolve alerts"]
        A2["Disk over 80%: alert"]
        A3["Upload-error alerts quiet for 48 h: resolve"]
    end
    subgraph Every_hour["Every hour (first run 30 s after start)"]
        B1["Retention: remove files deleted<br/>and versions pruned > N days ago;<br/>events older than 90 days"]
        B2["Garbage collection: delete chunks no<br/>version references, older than 24 h"]
        B1 --> B2
    end
    subgraph Tick["Every 15 s (FAFNIRX_TICK_SECS)"]
        C1["Claim one due source,<br/>run it"]
        C2["Claim one due mirror,<br/>run it"]
    end
```

- **Claiming** a source or mirror is one `UPDATE ... WHERE id = (SELECT ... FOR UPDATE SKIP LOCKED) RETURNING ...`, so a
  job can only be started once. A job left `running` by a crash is marked failed on the next start.
- **Garbage collection** takes an exclusive in-process lock for its final "is it still unreferenced" check and the
  delete; commits hold the same lock shared while they check chunks and record the version. That closes the window in
  which a client re-sends content whose chunk is about to be deleted.
- **Mirror runs** rebuild changed files from chunks into a staging directory (at most 512 MiB or 2000 files at a time),
  re-hash every chunk, then call `rclone`. Backend options are passed as environment variables so credentials never
  show up on a command line.

## Data on disk

```text
data/                            FAFNIRX_DATA
├── chunks/ab/cd/<64 hex>        content-addressed chunks (immutable)
├── tmp/                         in-flight writes; cleared on engine start
├── certs/                       ca.pem, ca.key, server.pem, server.key, hosts.txt
├── updates/<platform>/          bin, version, signature (written by fafnirx-sign publish)
├── mirror-stage/                temporary, during mirror runs
└── web_secret                   web UI session key
```

PostgreSQL's own files live separately (`FAFNIRX_DB_DIR`).

## Database structure

PostgreSQL, one schema, defined in [`schema.sql`](../schema.sql). Both processes apply it on start, serialized with an
advisory lock; every statement is idempotent. All integers are `BIGINT` (timestamps are unix seconds; `versions.mtime`
is unix nanoseconds).

```mermaid
erDiagram
    devices ||--o{ files : "owns"
    devices ||--o{ events : "has"
    files ||--o{ versions : "has"
    files }o..o| versions : "current_version_id"
    devices ||--o| sources : "hidden device (kind = source)"
    sources ||--o{ source_items : "imported items"
    mirrors ||--o{ mirror_items : "uploaded files"

    devices {
        bigint id PK
        text name UK "namespace"
        text kind "device or source"
        text token_hash "sha256 hex; '!' for sources"
        bigint created_at
        bigint revoked_at
        text hostname
        text os
        text ip
        text client_version
        bigint disk_total
        bigint disk_free
        bigint last_seen
        bigint last_collected
    }
    files {
        bigint id PK
        bigint device_id FK
        text path "UK with device_id"
        bigint current_version_id
        bigint deleted_at "set = marked deleted"
    }
    versions {
        bigint id PK
        bigint file_id FK
        bytea root_hash
        bigint size
        bigint mtime "unix ns"
        bigint mode
        bytea manifest "chunk hashes, 32 bytes each"
        bigint created_at
        bigint pruned_at "beyond version count"
    }
    events {
        bigint id PK
        bigint device_id FK
        text path
        text action "add update delete rename"
        bigint ts
        text hash
        text detail
    }
    alerts {
        bigint id PK
        text kind
        text key
        text message
        bigint created_at
        bigint updated_at
        bigint resolved_at
    }
    sources {
        bigint id PK
        text name UK
        text kind "gdrive"
        text config "JSON incl. credentials"
        bigint interval_secs
        bigint enabled
        bigint device_id FK
        bigint run_requested
        bigint last_run_at
        bigint last_ok_at
        text last_status "running ok partial failed"
        text last_message
        bigint created_at
    }
    source_items {
        bigint source_id PK
        text remote_id PK
        text path
        text remote_version
    }
    mirrors {
        bigint id PK
        text name UK
        text remote_type "rclone backend"
        text options "JSON incl. credentials"
        text remote_path
        bigint interval_secs
        bigint keep_runs
        bigint enabled
        bigint run_requested
        bigint last_run_at
        bigint last_ok_at
        text last_status
        text last_message
        bigint last_files
        bigint last_bytes
        bigint created_at
    }
    mirror_items {
        bigint mirror_id PK
        bigint device_id PK
        text path PK
        bytea root_hash
    }
    settings {
        text key PK
        text value
    }
```

### Tables

| Table | Purpose |
|---|---|
| `devices` | One row per client, and one hidden row per source (`kind = 'source'`, name `source:<name>`, token hash `!` which no token can match). The `name` is the namespace. |
| `files` | A path on a device, unique per `(device_id, path)`. `deleted_at` is the delete mark. `current_version_id` points at the live version (no foreign key, to avoid a cycle). |
| `versions` | One row per stored version of a file: root hash, size, mtime, and the `manifest`, the concatenated 32-byte chunk hashes in order. `pruned_at` marks versions beyond the version count; retention removes them later. |
| `events` | Change log shown on the device page (add, update, delete, rename). Kept 90 days. |
| `alerts` | `kind` is one of `device_offline`, `upload_error`, `disk_usage`, `source_failed`, `mirror_failed`. A partial unique index `(kind, key) WHERE resolved_at IS NULL` allows one open alert per cause. |
| `sources` | Configured third-party sources. `config` is JSON and holds credentials. The web UI sets `run_requested`; the engine writes the `last_*` fields. |
| `source_items` | What was already imported from a source: remote id to path and version marker, so unchanged and renamed items are not downloaded again. |
| `mirrors` | Configured offsite mirrors. Same pattern as sources. `options` is JSON and holds credentials. |
| `mirror_items` | The root hash of each file as last uploaded to a mirror. Deletes cascade with the mirror. |
| `settings` | Key/value: `version_count` (5), `retention_days` (30), `offline_after_secs` (60). |

Notes:

- Credentials in `sources.config` and `mirrors.options` are stored in plain text. Protect the database like the
  secrets it holds.
- To find the chunks of a version: `SELECT encode(manifest, 'hex') FROM versions WHERE id = ...`, split into 64-character
  hashes; the chunk file is `chunks/<h[0:2]>/<h[2:4]>/<h>`.
- The schema has no versioned migrations; see [Development](development.md#changing-the-schema).
