# Mirrors

A mirror is an offsite copy that the master keeps up to date with rclone. Administrators add them on **Mirrors**. The
database is not part of a mirror ([backups](deployment.md#backups)).

| | Backup | Replica |
|---|---|---|
| Files | Encrypted with your key; names too | As they are |
| Use for | Cloud storage you do not control | A NAS or disk you browse, share or copy back from |
| Restore | `rclone` with the restore kit | Copy the files, with any tool |
| Needs | Two passwords (the key) | Nothing |

The kind is chosen when the mirror is added and cannot be changed: add another mirror.

## What a mirror holds

```text
files/users/<user>/<key>/...      current tree per folder (a device's: <user>/<device>/<directory>)
files/<name>/<path>              master folders
versions/<UTC time>/...          what that run replaced or removed
meta/stats.json                  run stats
meta/<folder key, / as _>.json    file lists (path, size, mtime, root hash)
meta/library.json                albums, tags, favorites, ratings, descriptions, corrected dates and places
```

A backup has the same tree below the encryption: every name above, `files/` and `meta/` included, is encrypted on the
storage. `meta/library.json` is left out when the mirror holds one folder (it names files of the whole tree).

**What to copy:** everything, one whole folder (`master:<folder>`, `<user>/<folder>`), or one device with all its folders (`<user>/<device>`).
Files keep their place below `files/`. The choice cannot be changed later. A mirror points at the folder itself, so renaming
the folder keeps it attached; the next run copies it under the new name, and the copy under the old name stays on the storage
until you delete it there.

## How a run works

- Only changed files are sent. The master knows what each mirror holds (root hash per path), so a run **reads nothing
  back** from the storage: no egress, only a listing of `versions/` to prune. Each file is re-hashed before upload; a corrupt
  one makes the run **partial** (alert) and is not mirrored.
- A **rename or move** is a `moveto` on the mirror (server-side on most storage): the file is matched by content to the
  path that vanished. If the move fails the file is uploaded and the old path removed.
- A mirror **never deletes**. A replaced or deleted file moves to `versions/<run>/`; only the oldest runs beyond
  **Runs of history kept** (3) are purged.
- **Run now** starts a run; the circular arrow forgets what was uploaded (use it after emptying the storage).

## Add a backup to Backblaze B2

1. In B2 create a private bucket and an **application key** limited to it, with read, write and delete (the mirror prunes
   old `versions/` runs itself).
2. **Mirrors → Add mirror → Backup → Backblaze B2.** Enter the key ID, application key and `bucket/folder`.
3. **Generate a key**, then **Save as file** and store it away from the master. Tick the confirmation.

Encryption is set up by the master; you need nothing in `rclone.conf`. The key is two passwords: rclone's
crypt *password* and *salt*. Both are kept in the master's database in plain text (like every mirror credential), so
protect its backups.

### Restore

On the mirror's card, **Download restore kit** (administrators) gives an `rclone.conf` with the storage and the key. On any device with rclone:

```text
rclone --config fafnirx-restore.conf copy fafnirx-backup:files ./restored
rclone --config fafnirx-restore.conf ls fafnirx-backup:versions     # older versions, if you need them
```

If you lost the master, the key you saved is enough: write the config by hand.

```ini
[storage]
type = b2
account = <key id>
key = <application key>

[fafnirx-backup]
type = crypt
remote = storage:bucket/folder
password = <rclone obscure of the password>
password2 = <rclone obscure of the salt>
filename_encryption = standard
directory_name_encryption = true
```

`rclone obscure <text>` prints the form rclone wants in the file. **Without the key a backup cannot be read by anyone.**

### Cost and egress

Backblaze charges for storage, not for uploads. Downloads (a restore) are free up to three times the stored volume per month.
A run does listing calls only on `versions/`, so the number of paid transactions stays small.

## Add a replica

**Mirrors → Add mirror → Replica,** pick the storage and its details, no key. The files are readable where they are:
restore by copying them back.

## Storage

| Choice | rclone backend | Needs |
|---|---|---|
| Backblaze B2 | `b2` | Key ID, application key, `bucket/folder` |
| S3-compatible | `s3` | Provider, access key, secret, optionally endpoint and region |
| SFTP / SSH | `sftp` | Host, user, password, folder |
| WebDAV / Nextcloud | `webdav` | URL, user, (app) password |
| Folder or drive | `local` | A path on the master |
| Other rclone backend | any | The backend's name and its settings as `key=value` lines |
| Remote from rclone.conf | any | A remote you set up yourself, as `name:folder` |

Anything else rclone supports works through **Other rclone backend** or **Remote from rclone.conf** (settings as `key=value`
lines, as rclone names them in lowercase). OAuth storage (OneDrive, Dropbox) needs a token: create the remote once with `rclone config`,
put the `rclone.conf` in `./config` (or `FAFNIRX_RCLONE_CONFIG`; writable by the master, readable by its owner only) and use
it as a *remote from rclone.conf*. A backup on such a remote is layered over it too.

Options and passwords are never shown again. SFTP and WebDAV passwords are scrambled with `rclone obscure` by the master
before they are stored.

## Alerts

A failed or partial run raises **Mirror failed**; a good run clears it.

See also [Sources](sources.md), [Deployment](deployment.md).
