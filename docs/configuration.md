# Configuration file

What you decide (settings, devices, mirrors, sources) lives in one JSON file, `fafnirx.json`. You can edit it by hand,
keep it in version control, and mount it into the container, so that a master is set up again from it in one step.
The web app changes the same data. **The file is read when the master starts; the web app writes it as you go.** A
change in the web app is written to the file within a moment. An edit of the file takes effect at the next restart
(`docker compose restart master`). If the file has a mistake, the master refuses to start and says what is wrong, so
a bad file is found at once and has to be fixed, and a running master is never half-changed by an edit.

The database keeps what is a consequence of running: files and their versions, run results, device activity, albums
and tags. The file keeps the decisions. To rebuild a master, start a new database with the same file: devices keep
their tokens (the file holds each token's hash), mirrors and sources are back, and the files come back from the
devices (or from a mirror, see [restoring](usage.md#restoring-data)).

## Where it is

| Variable | Default | |
|---|---|---|
| `FAFNIRX_CONFIG` | `<FAFNIRX_DATA>/fafnirx.json` (`/config/fafnirx.json` in Docker) | The file |
| `FAFNIRX_RCLONE_CONFIG` | `rclone.conf` next to it | The rclone config file for mirrors that name a `remote` |
| `FAFNIRX_CONFIG_POLL_SECS` | `2` | How often the database is checked for changes to write to the file |

With Docker Compose the folder `./config` (`FAFNIRX_CONFIG_DIR`) is mapped onto `/config`. The repository ships the
folder; it must be writable by `FAFNIRX_UID` (`chown 1000:1000 config`), or Docker makes it for root when it is missing
and the master cannot write the file. The **folder** is mapped, not the single file: a file that is a mount of its own
cannot be replaced, and the master then has to write it in place. `config/fafnirx.example.json` shows every field; it
is a reference, not something to copy as it is (its device has a placeholder hash and the master would refuse it).
You do not have to write the file at all: start with nothing and the master writes `fafnirx.json` itself, then keeps it
up to date as you use the web app.

### Moving a master that already runs

```sh
node scripts/export-config.mjs              # reads the running database, writes config/fafnirx.json
docker compose up -d --build master         # the new master reads the file at start
```

`scripts/export-config.mjs` needs only Node. It reads the compose service `db` (`--container NAME` to use `docker exec`
instead, `--out FILE` for another path, `--force` to replace a file that exists) and works on a database made by an
earlier version. Devices keep their tokens, because the file takes over each token's hash.

If there is no file (or it is empty) at start, the master writes one from the database. If the file is there, it is
applied before anything else starts, and **a file that cannot be applied stops the master** with the reason; the file is
left exactly as you wrote it. Fix it and start again.

If you edit the file while the master runs, the master notices, logs that a restart is needed, and **stops writing to
the file** so that your edit is not overwritten. Changes made in the web app meanwhile are lost at the restart: the file
wins. So restart soon after editing, and do not use the web app to change the same things in between.

## The file

```json
{
  "version": 1,
  "settings": { "version_count": 5, "retention_days": 30, "offline_after_secs": 60 },
  "devices": [
    { "name": "laptop", "token_hash": "9f86d0..." },
    { "name": "desktop", "token": "a-token-you-chose" },
    { "name": "old-phone", "token_hash": "ab12...", "revoked": true },
    { "name": "photos", "kind": "local" }
  ],
  "mirrors": [
    { "name": "offsite", "remote_type": "s3", "remote_path": "my-bucket/fafnirx",
      "options": { "provider": "AWS", "region": "eu-west-1", "access_key_id": "...", "secret_access_key": "..." },
      "interval_hours": 72, "keep_runs": 3 },
    { "name": "vault", "remote": "b2crypt:fafnirx", "interval_hours": 24, "keep_runs": 10, "enabled": true }
  ],
  "sources": [
    { "name": "drive", "folders": ["1AbCdEf..."], "service_account": { "client_email": "...", "private_key": "..." }, "interval_hours": 24 }
  ]
}
```

| Section | Fields |
|---|---|
| `settings` | `version_count` (1-1000), `retention_days` (1-3650), `offline_after_secs` (5-86400). A setting left out keeps its value |
| `devices` | `name`; `kind` `device` (default, a client with a token) or `local` (a folder on the master); `token_hash` (hex SHA-256 of the token) or `token` (the token itself, to choose one by hand: it is replaced by its hash as soon as the file is read); `revoked` |
| `mirrors` | `name`; **either** `remote` (a remote of the rclone config, as `name:path`) **or** `remote_type` + `remote_path` + `options`; `interval_hours` (0.25 to a year, default 72); `keep_runs` (1-100, default 3); `enabled` (default true) |
| `sources` | `name`; `kind` `gdrive`; `folders` (ids or folder URLs); `service_account` (`client_email`, `private_key`); `interval_hours` (default 24); `enabled` |

A misspelled key is an error, not ignored. Names follow the same rule as in the web app (lowercase letters, digits,
`.`, `_`, `-`).

## What happens to what the file does not say

* **A section that is not in the file** (no `"mirrors"` at all) is not managed: the database keeps what it has, and
  the section is added to the file when the master starts.
* **A mirror or source that is no longer listed** is removed. A removed source's files go to the trash like any
  deleted files and are removed for good after the retention period. A mirror's data on the remote is not touched.
* **A device that is no longer listed** is **revoked**, never deleted: it cannot send any more, and everything it sent
  stays. (It then shows in the file as `"revoked": true`.) Local folders that are not listed stay: they are folders on
  the disk, and a folder dropped into `files/` is added to the file by the master.
* **A mirror that points somewhere else** (a new `remote`, `remote_type`, `remote_path` or option) starts from an empty
  remote, so it sends everything again on its next run. Changing only the schedule, `keep_runs` or `enabled` does not.
* **Run results** (last run, status, run now) are not in the file. They belong to the master.

## Mirrors that use an rclone remote

If you already have rclone remotes (an encrypted `crypt` remote, a `b2` bucket with an application key, an OAuth login
that `rclone config` made), use them: put `rclone.conf` next to `fafnirx.json` and name the remote.

```ini
# config/rclone.conf
[b2]
type = b2
account = ...
key = ...

[b2crypt]
type = crypt
remote = b2:my-bucket/fafnirx
password = ...      # from: rclone obscure
```

```json
{ "name": "vault", "remote": "b2crypt:" }
```

`remote` is `name:path`; the mirror writes `files/`, `versions/` and `meta/` below that path. Keep `rclone.conf` writable by
the master: rclone refreshes tokens in it. It holds credentials, like `fafnirx.json`, so keep both readable by their
owner only. In the web app, the same thing is the **rclone remote** field of **Add mirror**.

## Security

`fafnirx.json` holds credentials in the clear (mirror options, the Drive key), as the database does. The master
writes it readable by its owner only; do not commit it. Device tokens are not in it, only their hashes.
