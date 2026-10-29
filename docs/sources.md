# Sources

A source is storage the master reads on a schedule. Its files land in their own folder and appear under **Files** like a
device's: read-only (browse, download, hide), with versions and retention. A source never writes back.

A source belongs to its user (`<user>/<name>`). Its files are at `files/users/<user>/source-<name>/<path>`, the same
layout as every other device, so [mirrors](mirrors.md) and the network drive treat them alike. Credentials are never shown
again.

| Source | For | Needs |
|---|---|---|
| Google Drive | Folders shared with a service account | JSON key, folder ids |
| Other storage (rclone) | OneDrive, Dropbox, Nextcloud/WebDAV, SFTP, S3-compatible, a local folder | The storage's details |

## Other storage

**Sources → Add storage source:** name, storage, schedule. The choices and fields are the same as for a mirror
([Storage](mirrors.md#storage)).

- Each run lists the folder recursively and downloads files that are new or whose size or modification time changed.
- The remote layout is kept: `Photos/2024/a.jpg` on the storage is `source-<name>/Photos/2024/a.jpg`.
- The storage has no stable file ids, so a **rename or move downloads the file again** and the old path is marked deleted.
  Google Drive sources have ids and rename without a download.
- Files gone from the storage are marked deleted (kept for the retention period). If listing fails nothing is deleted and
  the run fails with an alert.
- Names rclone lists that cannot be a path (`..`, empty parts) and files of unknown size are skipped and counted.
- A file deleted in the web app stays deleted until it changes on the storage.

OAuth storage (OneDrive, Dropbox): run `rclone config` once, put the resulting `rclone.conf` in `./config` (or
`FAFNIRX_RCLONE_CONFIG`; writable by the master, readable by its owner only) and choose **Remote from rclone.conf**, written
as `name:folder`.

## Google Drive

1. Create a Google Cloud **service account**, download its JSON key, enable the Drive API.
2. Share each Drive folder with its email (viewer is enough).
3. **Sources → Add Google Drive source:** name, folder ids or URLs, key, interval.

- Raw files only; Docs, Sheets, Slides, forms and shortcuts are skipped and counted.
- Names are made safe (`/ \ : * ? " < > |` → `_`); duplicates get ` (<id prefix>)`.
- A Drive rename/move is a `rename` without a download.
- Files gone from Drive are marked deleted; if listing fails nothing is deleted. Failed or partial runs raise an alert.

## Run and pause

**Run now** starts a run at once; the pause button stops the schedule; deleting a source keeps its files for the retention
period, then removes them. A failed or partial run raises **Source failed**; a good run clears it.

See also [Mirrors](mirrors.md), [Usage](usage.md).
