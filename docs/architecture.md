# Architecture

```mermaid
flowchart TB
    C["Client (devices)"] <-- "gRPC :7443" --> M
    W["Web app (browser)"] <-- "JSON + SSE :8080" --> M
    subgraph M["fafnirx-master (one process)"]
        direction LR
        G["Device API"]
        A["Web API"]
        B["Background work: watcher, jobs, sources, mirrors"]
    end
    M --> PG[("PostgreSQL")]
    M --> FS[("files/ and versions/")]
    PG -. "LISTEN / NOTIFY" .-> A
```

| Component | Role |
|---|---|
| Client `fafnirx` (`crates/client`) | Watch, hash in chunks, upload, offline queue, shared-folder sync, tray, self-update from GitHub releases |
| Master `fafnirx-master` (`crates/master`) | Device API, web API, file store, ingest, retention, alerts, sources, mirrors, metadata |
| Web app (`frontend`) | React, built into the master. A module's part is `src/modules/<id>`, built apart and loaded by the browser from the module while it is up (`src/sdk`, `src/lib/module-loader.ts`) |
| Modules (`modules/<id>/service`) | Optional services, run by the master as plugins (`<data>/plugins/<id>`) or as containers; only the master talks to them ([modules](modules.md)). Today: `fafnirx-video`, HLS streaming and video details ([video](video.md)) |
| `crates/module-api` | The contract between the master and a module: manifest, capabilities, version |
| `fafnirx-sign` (`crates/sign`) | Optional: signed client builds |
| `crates/common` | gRPC types, chunking, hashing, path rules, reading dates and places from metadata text |

## Design decisions

- **Two APIs, two trust levels.** The device gRPC API (`proto/fafnirx.proto`) has no call that reads, lists or deletes the
  files of a device; `Negotiate` only reports which chunks of the caller's own file are missing. The one exception is
  [shared folders](#shared-folders), which a device reads and writes for its own user. Everything else that reads or deletes is
  in the web API, behind a login and role checks. Different ports, no shared authentication.
- **Plain files.** A path's current version is stored whole in `files/users/<user>/<device>/<path>` (`files/<path>` for master
  files). A replaced or deleted version moves to `versions/<version id>/<user>/<device>/<path>`. Chunks exist only on the wire
  and in `tmp/`; there is no cross-file deduplication.
- **Users own devices; the name is the folder.** `devices.name` is an immutable slug (1-63 of `a-z 0-9 . _ -`, no reserved
  Windows names), unique per user; `devices.folder` (`<user>/<name>`, set by trigger) names a device in the store, mirrors and
  API. Master files are one internal device (kind `master`, id 0, folder `_master`, no owner). Local folders (`local`),
  sources (`source`) and shared folders (`shared`) are devices too.
- **IDs.** Files, albums, tags, jobs and alerts have random v4 UUID keys, shown as 22-char URL-safe Base64 without padding
  (`platform/id.rs`, `Id`; the hyphenated form is accepted). Devices, users, sources, mirrors are addressed by slug. Versions
  (named in `versions/`), events (ordered) and sessions keep numeric keys; jobs add `seq` for queue order.
- **One process.** Locks, live events and queued work go through PostgreSQL. Only browser uploads in progress and the login
  rate limit live in memory.
- **A client knows its master.** Each database has a random `settings.master_id`, asked at the start of every cycle
  (`Identify`); a different id makes the client drop its record and resend everything.
- **Metadata is separate from data.** Losing PostgreSQL loses versions and history, not the files; losing the data directory
  loses the data. Back up both.
- **No TLS in the master.** Trust comes from the network or a TLS proxy. Tokens are hashed, passwords use argon2.
  `FAFNIRX_TRUSTED_PROXY` names the proxies whose `X-Forwarded-For` is believed.
- **Library data sits beside the file.** `file_meta` is per content (root hash); `file_state` per file and user; albums, tags
  and states go with their file (`ON DELETE CASCADE`).
- **Albums** are picked files (`album_files`), a folder (`folder_device_id` + `folder_prefix`: every live photo and video under
  the prefix), or both; one folder album per user and folder. The cover is decided in `library/albums.rs` (chosen, poster,
  first video, newest). `repo::albums::follow_folder_move` changes the prefix (and an auto-name) in the rename's transaction
  when the last file leaves a folder to a sibling of the same parent.
- **Artwork is looked up, not stored.** `library/artwork.rs` picks a video's picture (own, else the poster of its folder or
  the two above) and `.nfo` from `repo::files::neighbours`; `thumb` and the on-demand picture use the same lookup, cached by
  the image's content hash.

## What a file operation means

One operation, one event, one transaction under the path's lock.

| Operation | `files` row | Event | Trash |
|---|---|---|---|
| First write of a path | created, version current | `add` | no |
| New content for a live path | new version current, old archived | `update` | no |
| Rename by a client or source (`commit_version`), a move noticed in a local folder (`adopt_live`), or Move in the web app (`move_path`; local folders, shared folders and master files only, the file is moved not copied) | new path written; old path vacated: `deleted_at` set, `deleted_by='rename'` | one `rename` on the new path, detail = old path | **no**: hidden from Trash, listings, deleted counts; retention removes it |
| Delete (device, source, web app, file gone from the store) | `deleted_at` set (`deleted_by='web'` in the web app) | `delete` | yes |
| Brought back (file returns, version restored) | `deleted_at`, `deleted_by` cleared | `add`/`update` | leaves |

Retention, jobs and sign-ins write no events. Per-day activity counts `add`, `update`, `rename`, `delete`.

## Upload and commit

```mermaid
sequenceDiagram
    participant C as Client
    participant M as Master
    participant DB as PostgreSQL
    participant FS as Files
    C->>M: Identify (each cycle)
    M-->>C: master id
    C->>M: Negotiate(path, size, chunk hashes, root)
    M->>DB: same content live under another path?
    M->>FS: which chunks are in tmp/?
    M-->>C: missing chunk indexes
    C->>M: UploadChunks (parallel)
    M->>FS: verify hash, write at index * 4 MiB
    C->>M: Commit(manifest, mtime, mode, renamed_from?)
    M->>DB: BEGIN, advisory lock (device, path) [both paths of a rename]
    M->>FS: re-hash, archive current file, move new file into files/
    M->>DB: version, event, meta job, NOTIFY, COMMIT
    M-->>C: version id
```

Every call carries the token and `x-fafnirx-instance`. If anything fails after files moved, the moves are undone before the
error returns.

**Locking.** Every change to a path (commit, delete, web upload or move, watcher) first takes a PostgreSQL advisory
transaction lock on `(device id, path)`; a rename takes both paths in a fixed order. File moves happen inside the lock, so
paths never interleave and the watcher never sees a half-done commit.

**Watcher.** `files/livetree.rs` watches `files/`. A path quiet for 5 s is compared with the database: new or changed files
become versions (`adopt_live`), vanished ones are marked deleted (`forget_absent`), a vanished file matched by a new file of
the same size and mtime is a rename, a new `files/users/<user>/<name>` becomes a `local` device. The whole tree is also compared
every 5 minutes or on `POST /api/files/rescan`. A missing device folder deletes nothing.

**Live updates.** `db::notify(topic, key)` runs `pg_notify` in the changing transaction, so it arrives on commit. One
`LISTEN` connection feeds browsers over SSE (`GET /api/events`); the web app maps topics (`files`, `library`, `activity`,
`devices`, `alerts`, `jobs`) to stale queries and refetches once per burst. A slow stream gets `resync`.

## Shared folders

A shared folder is a `devices` row of kind `shared`: owned by a user, no token, folder `<owner>/<name>`. Versions, trash,
retention, mirrors, the watcher and the web file browser treat it like a local folder. `shared_members` lists the other users
the owner added, each read-write or read-only; a member leaves with `POST /api/shared/leave`. The web app and the clients
name it `<owner>/<name>`. Clients reach it like this:

- **Writes.** `Negotiate`, `UploadChunks`, `Commit` and `MarkDeleted` act on the shared folder instead of the calling device
  when the call has `x-fafnirx-shared: <owner>/<name>` (the owner may use the bare name). The caller still authenticates as its
  own device and keeps its lease. `acting()` in `devices/grpc.rs` resolves the header among the folders the device's user owns
  or was added to, and refuses writes from a read-only member. Without the header a call is a plain upload, unchanged.
- **Reads.** `ListShared`, `ListChanges(shared, since)` and `DownloadShared(shared, path)`; `CreateShared` makes one.
  `ListChanges(shared, since)` keeps no log: paths written or deleted at or after `since`, as they are now
  (`files::changes_since`), plus a cursor (server time - 2 s). A deletion stays listed while the trash holds the row, and
  carries `from_web` (`deleted_by = 'web'`): the client applies those without the delete guard.
  `settings.deletions_known_from` (retention, forward only) is how far back all deletions are known; a caller with an older
  cursor, or none, gets every live path with `complete` set, and the client treats in-step paths not listed as deleted.
- **Wake-up.** `WatchShared(names)`: one message per folder per change burst (`WATCH_SETTLE`), fed by the `NOTIFY` broadcast
  of SSE (`state.events`). The client keeps one open; 60 s poll is the fallback.
- **Names.** `ingest::check_names` (commits, moves into the folder) refuses: names Windows cannot hold
  (`common::not_portable`), a path differing only in case from a live one (`files::case_clash`), a folder above it that does
  (`files::folder_spellings`). Lookups use the `files_folded` index; `common::fold_path` is the client-side equivalent.
  Updating an existing path is allowed; a rename's source does not count (case-only renames work). Client:
  `respell_folders` renames the folder on a received case rename (case-folding file systems keep the old spelling);
  `exists_spelled` deletes only a path spelled as it was. `local` and `master` folders (SMB-visible) get `check_names` only
  with `from_web`; watcher, adoption and device backups are taken as is.
- **Web access.** `Access::shared_with_me` (loaded with the session) gives members visibility, write if allowed. File
  queries call SQL `visible(device, user, everything)`: master files, own devices, shared folders added to.
  `purge_shared_folder` needs all files deleted first, then removes trash, versions, log, stored copies.
- **Client sync.** `client/src/shared.rs`: one `Attached` per `[[shared]]` entry, each with its own state DB. Receive, then
  send through the upload `Engine` (`Engine::group = at_the_top`: the folder is the path root). A local file matching its
  recorded (size, mtime) is unchanged, so the master's change applies; otherwise hash: equal = in step, different = conflict
  (local file renamed `name (conflict <computer> <date>).ext`). Receive writes `.fafnirx-part-*`, verifies the root hash,
  renames. `Spellings` reads each directory once per pull for case-clash lookups.
- **Client state.** The state DB is keyed to the folder name and the local path (`adopt_folder`); another path starts empty.
  `status` reads only `state::State` (synced paths, holds, last cycle and problem), so it works with the client stopped.
- **Held deletions.** A large deletion by another computer (not `from_web`) is held in `pull_hold`, without expiry.
  `release_deletes` applies it; `keep_deletes` drops the files from the in-step records so they are re-sent, and the cursor
  moves past. The heartbeat carries per-folder state; the master raises `deletions_held` per computer
  (`alert_held_deletions`), expired by the minute job after 15 min without renewal.

## Background work

| Work | Where | Notes |
|---|---|---|
| Offline and disk alerts, upload-error and held-deletion expiry | `monitoring/jobs.rs`, every minute | |
| Retention, version pruning, stale uploads, cache cleaning | `monitoring/jobs.rs`, hourly | Removes files with their records |
| Sources, mirrors | `integrations/` | `FOR UPDATE SKIP LOCKED` |
| `meta` → `artwork` → `thumb` | `library/meta.rs`, table `jobs` | 3 attempts; videos are probed and drawn by a module (`modules/`, capabilities `probe.video`, `poster`); a job returns a note shown in Processing |
| Pictures on request | `library/meta.rs` | `cache/thumbs/`, at most 4 decoded at once |
| Segments, subtitles | `modules/video/service` | On request; in memory, `GET /status` |

`done` jobs stay a day; queueing the same `(kind, payload)` while one is queued or running is a no-op; setting `jobs_paused`
makes workers wait. A file counts as unread while it has no `file_meta` row: that is the API's `processing` flag; `work`
(`waiting`, `running`, `failed`) comes from its unfinished jobs.

## Database

```mermaid
erDiagram
    users ||--o{ devices : owns
    users ||--o{ albums : makes
    users ||--o{ tags : makes
    users ||--o{ file_state : says
    devices ||--o{ files : owns
    devices ||--o{ events : logs
    files ||--o{ versions : has
    files ||--o| file_state : "what people say"
    files }o--o{ albums : album_files
    files }o--o{ tags : file_tags
    versions }o--o| file_meta : "same root hash"
    files {
        uuid id PK
        bigint device_id FK
        text path
        bigint current_version_id
        bigint deleted_at
        text deleted_by "null, web, rename"
    }
    events {
        bigint id PK
        text action "add, update, rename, delete"
        text detail "old path of a rename"
    }
    albums {
        uuid id PK
        uuid cover_file_id FK
        bigint folder_device_id FK
        text folder_prefix
    }
    jobs {
        uuid id PK
        bigint seq "queue order"
        text kind "meta, artwork, thumb, proxy"
        text note
    }
```

Other tables: `users`, `sessions`, `devices`, `shared_members`, `device_shared_state` (what each computer says about its shared
folders, replaced with every heartbeat), `versions`, `file_meta`, `file_state`, `tags`, `alerts` (uuid),
`settings`, `device_activity`, `sources`, `source_items`, `mirrors`, `mirror_items`, `schema_migrations`. The migrations in
`crates/master/migrations/` are the source of truth. `events` are kept 90 days.

**Indexes** (migrations `0001`, `0002`, re-made in `0010`): `files (device_id, path)` unique, `(device_id, path
text_pattern_ops)` for folders, `path` trigram (`pg_trgm`) for search, `current_version_id`, `deleted_at` where deleted, `id`
where live and `media` set; `versions (file_id, id)`, `root_hash`; `events (device_id, id)`, `(device_id, path, id DESC)`,
`ts`; `devices.token_hash` unique, `folder` unique; `jobs (kind='proxy', seq)` where queued, `(status, kind)`; `alerts
(kind, key)` unique where open; both directions of each link table. `files.media` is a column computed from the file name
(a test keeps it equal to `library/meta.rs`).

**Device totals** (`file_count`, `total_bytes`, `deleted_files`, renamed-away rows excluded) are kept on the device row,
marked stale by writes and recomputed when the device finishes syncing (or within a minute for devices that do not report).

## Web API

JSON under `/api` behind the login cookie (random token, hash stored, `SameSite=Strict`, `HttpOnly`). Changing calls need
`x-requested-with`. Ids are the 22-character form.

| Area | Endpoints |
|---|---|
| Session, users | `POST /api/login`, `/logout`, `GET /api/session`, `POST /api/me/password`; admin: `GET/POST /api/users`, `PATCH /api/users/:name` |
| Overview | `GET /api/dashboard`, `POST /api/alerts/:id/dismiss`, `GET /api/events` (SSE) |
| Devices | `GET/POST /api/devices`, `GET /api/devices/:folder` (slash as `%2F`), `POST .../token`, `.../release`, `.../revoke` |
| Shared folders | `GET/POST /api/shared`, `DELETE /api/shared/:name` (409 while it has files; then removes the rest), `POST /api/shared/:name/members` (`{user, can_write}`), `DELETE .../members/:user`; only the owner manages a folder. Members use it through `/api/files` like any folder |
| Files | `GET /api/files` (entries carry `id`, `processing`), `GET /api/files/info` (carries `work`), `POST /api/files/delete`, `/restore`, `/move` (`{device, from, to}`; local folders, shared folders and master files, 409 otherwise), `/rescan` (admin) |
| Transfers | `GET /api/files/download` (ranges, versions), `/thumb`, `POST /api/files/zip`, `POST /api/uploads`, `PUT/GET/DELETE /api/uploads/:id`, `POST /api/uploads/:id/finish` |
| Library | `GET /api/timeline/buckets`, `/timeline?month=`, `/library?ids=`, `GET/PATCH /api/library/:id` (carries `work`), `GET /api/library/:id/folder`, `POST /api/library/bulk`, `GET /api/tags`, `DELETE /api/tags/:id`, `GET /api/map`, `GET /api/trash`. Assets carry `processing` |
| Pictures | `GET /api/media/:id/{thumb,preview,original,download}` (`?v=<hash>` caches for good) |
| Albums | `GET/POST /api/albums` (a `folder` body makes or returns the folder's album), `GET/PATCH/DELETE /api/albums/:id`, `POST .../files` and `.../files/remove` (by `ids` or `items` of `{device, path}`) |
| Modules | `GET /api/m/:module/:id/*rest` asks the module for `/files/<rest>` about the file (login and right to the file first); video: `hls/{master.m3u8,<quality>/index.m3u8,<quality>/seg<n>.ts,subtitles.json,subtitles/<n>.vtt}`. `GET /api/m/:module/ui/:file` their web part. `GET /api/session` lists the modules that are up, `GET /api/queue` carries their reports |
| Plugins (admin) | `GET /api/plugins` (installed, with state), `GET /api/plugins/available` (what the release of this version carries), `POST /api/plugins/:id` (install or update), `DELETE /api/plugins/:id` |
| Processing | `GET /api/queue`; admin: `POST /api/queue/pause`, `/retry`, `/cancel`, `/rebuild` |
| Integrations | `GET/POST /api/sources`, `POST /api/sources/rclone` (any rclone storage), `/:user/:name`, `/run`, `/toggle`; admin: `GET/POST /api/mirrors` (`kind` replica or backup) + `/:name`, `/run`, `/toggle`, `/resync`, `GET /api/mirrors/:name/restore-kit` (backups), `GET /api/mirrors/scope?path=` (folders to pick from) |
| Settings | `GET/PUT /api/settings` |

Browser uploads are chunked: pieces are `PUT` at an offset (409 with the expected offset if misaligned); `finish` verifies the
hash and commits through the device path.

## Security model

| Threat | Mitigation |
|---|---|
| Stolen device token | Adds data to its own device, and reads and writes the shared folders of its user (replaced files stay as versions); revoke it. A second computer is refused while the first is connected, and the owner is alerted |
| Compromised device | Cannot read, list or delete anything but the shared folders of its user and those shared with them |
| Stolen password | Guesses rate-limited per address and user; sessions end on disable, new password, logout. Add Access or a VPN |
| A user reaching another's files | Queries and downloads are limited to the role's devices (`web/access.rs`); others' devices answer 404. Ids are random but never trusted: access is checked on every call |
| Compromised master | Cannot ship a client update: GitHub, or builds signed with a key not on the master |
| Path tricks | Validated on arrival and again when mapped to disk |
| Stored HTML/SVG opened in a browser | Always a download with `nosniff` |
| Tampered chunk, bit rot | Verified on arrival and before install; re-hashed before mirroring (a mismatch fails that file) |
