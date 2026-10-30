# Sources

A source is storage the master reads on a schedule. Its files land in their own folder and appear under **Files** like a
device's: read-only (browse, download, hide), with versions and retention. A source never writes back.

A source belongs to its user (`<user>/<name>`). Its files are at `files/users/<user>/source-<name>/<path>`, the same
layout as every other device, so [mirrors](mirrors.md) and the network drive treat them alike. Credentials are never shown
again.

Sources read any storage [rclone](https://rclone.org) supports: Google Drive, OneDrive, Dropbox, Nextcloud/WebDAV, SFTP,
S3-compatible, a local folder.

**Sources → Add storage source:** name, storage, schedule. The choices and fields are the same as for a mirror
([Storage](mirrors.md#storage)), plus Google Drive.

- Each run lists the folder recursively and downloads files that are new or whose size or modification time changed.
- The remote layout is kept: `Photos/2024/a.jpg` on the storage is `source-<name>/Photos/2024/a.jpg`.
- The storage has no stable file ids, so a **rename or move downloads the file again** and the old path is marked deleted.
- Files gone from the storage are marked deleted (kept for the retention period). If listing fails nothing is deleted and
  the run fails with an alert.
- Names rclone lists that cannot be a path (`..`, empty parts) and files of unknown size are skipped and counted.
- A file deleted in the web app stays deleted until it changes on the storage.

OAuth storage (OneDrive, Dropbox): run `rclone config` once, put the resulting `rclone.conf` in `./config` (or
`FAFNIRX_RCLONE_CONFIG`; writable by the master, readable by its owner only) and choose **Remote from rclone.conf**, written
as `name:folder`.

## Google Drive

1. Create a Google Cloud **service account**, download its JSON key, enable the Drive API.
2. Share the Drive folder with its email (viewer is enough).
3. **Sources → Add storage source → Google Drive:** the key, the folder id (the part after `/folders/` in its URL) and,
   if you want only a subfolder, its name.

One source reads one folder; add another source for each further folder. Docs, Sheets and Slides are skipped (add
`drive_export_formats` under *More backend options* to export them). Drive sources made before this changed are switched
off, with their files kept: add the folder again and delete the old source.

## Run and pause

**Run now** starts a run at once; the pause button stops the schedule; deleting a source keeps its files for the retention
period, then removes them. A failed or partial run raises **Source failed**; a good run clears it.

See also [Mirrors](mirrors.md), [Usage](usage.md).
