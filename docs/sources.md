# Sources

A source is storage the master reads on a schedule. Its files are written into a folder you choose and appear under
**Files** there like any other file, with versions and retention. A source never writes back.

A source belongs to its user (`<user>/<name>`). **Where the files go** is one of: a folder of its own (`<user>/source-<name>`,
a normal local folder, which you can rename), or any subfolder of one of your folders, a folder shared with you that you may change, or the
master files (not `users/`). The subfolder is made on the first run. Files are named below that place, so [mirrors](mirrors.md)
and the network drive treat them like any others. Credentials are never shown again.

**Edit** (pencil on the Sources page) changes the name, the place, the schedule and what it reads; a password or cookie field left
empty keeps what is stored. Moving or renaming the folder in Files, or its parent, keeps the source attached. Changing the place
takes what it collected along: within the same folder the files are moved (nothing else in the folder is touched, and nothing moves if a name is in the way); into another folder they go to the Trash and it collects everything again there. It cannot be changed while it runs. **Deleting** a source removes
only the files it collected, not the rest of the folder.

Sources read any storage [rclone](https://rclone.org) supports: Google Drive, OneDrive, Dropbox, Nextcloud/WebDAV, SFTP,
S3-compatible, a local folder.

**Sources → Add source** has two tabs: **Storage** (a protocol: Backblaze B2, S3-compatible, SFTP, WebDAV, a
folder, a remote from `rclone.conf`; the same choices and fields as for a mirror, see [Storage](mirrors.md#storage)) and
**Services** (a provider with its own form: Google Drive, read with rclone; web addresses and feeds, read by the [fetch module](fetch.md); and whatever [plugins](plugins.md) add). Then a
name and a schedule.

- Each run lists the folder recursively and downloads files that are new or whose size or modification time changed.
- The remote layout is kept: `Photos/2024/a.jpg` on the storage is `Photos/2024/a.jpg` below the place the source writes to.
- The storage has no stable file ids, so a **rename or move downloads the file again** and the old path is marked deleted.
  (A kind that gives its files keys, see [Plugins](plugins.md), moves them instead.)
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
3. **Sources → Add source → Services → Google Drive:** the key, the folder id (the part after `/folders/` in its URL) and,
   if you want only a subfolder, its name.

One source reads one folder; add another source for each further folder. Docs, Sheets and Slides are skipped (add
`drive_export_formats` under *More backend options* to export them). Drive sources made before this changed are switched
off, with their files kept: add the folder again and delete the old source.

## Web files and feeds

**Services → Web** (offered when the [fetch module](fetch.md) is running; read by it, not by rclone): addresses of files and/or an
RSS or Atom feed whose entries link to files, filters by name and size, and a confirmation that you may copy what it downloads. A file
that changes on the server comes as a new version; a feed only adds: what drops out of it is not deleted. Rename and delete work as
above. Details and legal notes: [Fetch module](fetch.md).

## More kinds of source

A [plugin](plugins.md) adds a kind of source of its own to **Services** (a site, a protocol, a service with an API): it brings its
form, and checks, lists and downloads on the master's behalf. The master treats its files like any other source's. Anyone can write
one and run it as a container next to their server: [Plugins](plugins.md).

## Run and pause

**Run now** starts a run at once; the pause button stops the schedule; deleting a source keeps its files for the retention
period, then removes them. A failed or partial run raises **Source failed**; a good run clears it.

See also [Mirrors](mirrors.md), [Usage](usage.md).
