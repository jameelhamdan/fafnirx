# Usage

Log in at `http://<master>:8080`. A new master has the user `admin` with `FAFNIRX_ADMIN_PASSWORD`; change it from the user
menu (top right), which also logs you out. Pages update live.

| Page | Purpose |
|---|---|
| Dashboard | Alerts, totals, disk, online devices, activity, recent changes |
| Timeline, Favorites, Archive, Albums, Trash | Photos and videos by month and day; grouped; deleted files |
| Files | Browse, search, preview, download, upload, move, delete |
| Processing | Background work |
| Devices, Sources | Devices that send, with their folders and your other folders; storage that is read |
| Mirrors | Offsite copies (administrators only) |
| Users, Settings | Administrators only |

## Users and roles

| Role | Own devices and folders | Master folders | Others' files | Users, settings, mirrors |
|---|---|---|---|---|
| Administrator | read, write | read, write | read, write | yes |
| Normal | read, write | read, write | no | no |
| Read only | none | read | no | no |

- Master folders belong to nobody; everyone sees them.
- Users cannot be renamed (the name is a folder). Disabling one ends their logins. One administrator always remains.
- **Delete** (on a disabled user, not yourself or `admin`) removes the account with their devices and what those sent (the
  devices keep their files), memberships, tags, albums and favorites. Refused while one of their folders has files or they
  have a source, or a mirror or someone else's source uses their folder. The name can then be used again.
- A device belongs to the user who adds it; names are unique per user (`bob/laptop`, `alice/laptop`).
- Favorites, ratings, tags, albums, corrected dates and places are per user.
- Anyone who cannot see a file gets 404 for it.

Files, albums, tags, jobs and alerts have UUIDs, shown as 22 URL-safe characters. Useful links: `/watch/<id>` opens a photo or
video in the viewer (send it to anyone who can see the file); `/albums/<id>` an album.

## Files

The top level lists folders: those of users, the master folders and a folder for each directory a device sends, each named by its key (`user/name`, `master:name`, `user/device/directory`). A badge shows
**Changeable** (master folders and folders of users, if your role may write) or **Read-only** (what devices sent).

- **Search** (current folder and below), type filter, sort, deleted files, list or grid.
- **Select** to download (zip for several), show as album, move or rename, delete.
- **Details** show preview, EXIF and the **history**: every version can be downloaded or restored (restoring adds a new
  version and loses nothing). Photos and videos also have the library data (favorite, rating, tags, date, place).
- **Move or rename** works in folders of users and master folders. A folder moves with its contents, and albums and sources that point
  into it follow. Nothing moves if a name is in the way. Files a device sent cannot be moved (409).
- **Rename folder** (a top-level folder you own) changes its name everywhere at once; devices that sync it keep working.
- **Upload** into a folder of yours or a master folder by drag and drop (resumable).
- **Delete** moves a file to the Trash; data stays until retention ends or you choose **Delete forever**, which frees the disk.
  Files from a device or source are not re-fetched. In a synced folder the deletion reaches every device.

## Photos and videos

A view of the same files; originals are never changed.

- **Timeline** by month and day, filtered by device, kind and tag. **Viewer:** arrows or swipe, `Esc` closes, `i` info, `f`
  favorite, double-click or `z` zooms a photo (arrows then pan), **Copy link** in the menu.
- **Info:** description, rating, tags, date, place. Corrections are notes beside the file; **Use the original** reverts them.
- **Date and place** come from EXIF (then XMP; for a video, its tags). After an update that reads more, **Processing → Read
  everything again** re-reads the library.
- **Archive** hides from the Timeline. **Delete** goes to the Trash, restorable with albums, tags and favorite.

**Albums.** An album is a shortcut to a folder (**Show as album** on a folder in Files); it holds no files of its own. Everything
in the folder and below is in it, new files included, in natural order (`Episode 2` before `Episode 10`). It follows the folder
when it is renamed or moved. One per folder and user. The cover is the file you chose, else a poster image in the folder
(`poster`, `cover`, `folder`, `thumb`), else its first video or photo. An album page shows at most 20,000 files and says
so; the folder has the rest.

**Artwork.** A video's picture comes from files beside it in the usual Jellyfin/Kodi/Plex names (`<video>-poster.jpg`, or
`poster.jpg` in its folder or the two above); title, season, episode and plot from `<video>.nfo`. Nothing is downloaded. Without
a picture a video shows a frame from the [video module](video.md).

## Processing

The queue of background work for **photos and videos only**; other files are never read.

| Job | Does |
|---|---|
| `meta` | Reads details: EXIF for photos; duration, codec, date and place for videos (needs the [video module](video.md)) |
| `thumb` | Makes a thumbnail (320 px) or preview (1600 px) the first time one is needed; kept in `cache/thumbs/` |

`meta` runs when a file arrives. A job is tried 3 times, then fails; done jobs stay a day. Users see the jobs of
devices they can see. Administrators can **Pause**, **Retry** or **Clear** failed jobs, **Cancel** waiting ones, and **Process
missing**. Runs of sources and mirrors (jobs of their own: a failed run stays until the next one, **Retry** runs it again; a waiting one cannot be cancelled, its schedule would queue it again) and what modules are doing (files being fetched, segments being cut) are in the same list, each with the module that does it. **Settings → Thumbnails and previews** off: none are made.

## The change log

The Dashboard and device pages list what happened to files, kept 90 days: `add`, `update`, `rename` (with the old path) and
`delete`. A rename is one event and not a delete: the old path is hidden, not in the Trash, and removed when retention ends.
Metadata work, retention and sign-ins are not logged.

## Devices and folders

The **Devices** page lists the devices; open a row to see the folders attached to it (state, size, last sync). Below it are all
folders: yours, shared with you, and master folders (make, share, rename, delete). A folder that a source fills carries a
**Source: name** badge, and there is a filter for them.

**Add device** gives a name and a token (shown once). **Rotate token** replaces it. **Revoke** stops uploads and keeps the data.
**Release** lets another device take the device over.

One device holds a device at a time. A second one with the same token is refused while the first is heard from, and the owner
gets an alert. After 90 s of silence it may take over; Release or Rotate does it at once.

## The client

```sh
fafnirx install --server nas.lan                # copies the program, asks for the token, installs the service (see Installation)
fafnirx folder add ~/Documents --name documents # collect a folder (see Folders below for other targets)
fafnirx folder list | remove <name>             # remove stops syncing; the master keeps its data
fafnirx pause | resume
fafnirx status [--json]                         # what the client is doing (also when stopped)
fafnirx sync                                    # one cycle in the foreground
fafnirx release-deletes                         # apply held-back deletions now
fafnirx update [--check]                        # install the newest release, or only look
fafnirx restart                                 # restart the client and the tray
fafnirx uninstall [--purge]                     # remove the service, the program, and with --purge the config
fafnirx tray
```

The config is a TOML file made by `fafnirx install --server <master>` (`--config` selects another): `%LocalAppData%\fafnirx\config.toml` on Windows, `/etc/fafnirx/config.toml` on Linux,
`/Library/Application Support/fafnirx/config.toml` on macOS. The token is in the OS keychain. On macOS and Linux commands that
change the config ask for `sudo`; on Windows they do not.

```toml
server = "http://nas.lan:7443"  # https://... behind a TLS proxy
bandwidth_limit_kbps = 0        # 0 = unlimited
watch = true                    # react to file changes
scan_interval_secs = 3600       # full scan, 0 disables
delete_hold_hours = 12          # see the delete guard
auto_update = true              # check GitHub releases every 6 hours
web_url = "http://nas.lan:8080" # the tray's link to the web app
```

`[[folders]]` tables are written by `folder add`: each has a `path`, a `name` and a `target` (`device`, the default, or `folder`,
a folder of the master) and, for the second, a `mode` (`both`, the default, `up`, `keep` or `ask`; a `device` folder is always `up`)
and an `id`, set when attaching. With `ask`, `ask_timeout_hours` (default 24, 0 = for ever) is how long a file waits for an answer
before both versions are kept.

**Service.** On Windows the client belongs to your user: it and the tray start when you log in. On Linux and macOS it is a
service of the device (systemd, launchd) that runs as root, starts at boot, is restarted on failure, and reads every user's
folders. The tray of each user starts at login and talks to the service through `<data>/control`.

**What is collected:** everything under the added folders except `.fafnirxignore` matches (gitignore syntax). Symlinks, locked
files and case-only name clashes are skipped and logged. A vanished folder (unmounted drive) deletes nothing. The client never
writes to collected folders. While the master is down, changes queue.

**First sync.** Files are hashed and uploaded one at a time in 4 MiB chunks over parallel streams, within the bandwidth limit. A
stopped sync resumes where it was. If the master's database is replaced, the client notices and sends everything again.

### The delete guard

If at least 10 files of a folder vanish and they are over 20% of what was sent from it, the client holds the
deletions, raises an alert and keeps uploading. If the files come back nothing is deleted; after `delete_hold_hours` the
deletions are sent, or send them now with `fafnirx release-deletes`.

### The tray

Green is online, red an error, amber paused, grey not running. The menu shows what is happening and has pause/resume, sync now,
**Check for updates**, **Folders** (those of this device, and the folders of the master you can sync), and links to the config, log,
client data folder and web app. Every sync (**Sync now** too, at most every 10 s) asks the master for its folders; when a folder attached here is gone from
the master (deleted, or no longer shared with you) and its folder on this device is empty, **Folders** gets the entry
**Remove N folders deleted on the server**, which stops syncing them after you confirm. A folder that still holds files is never offered. On macOS and Linux the folder entries need no password: the tray leaves a request and the
service applies it, only for folders owned by the user who asked.

## Folders: sync both ways

Every folder of a device goes to a **target**, and its mode says which way. The default target, `device`, is a backup: up
only, the same as the mode `up`. A **folder** (made in Files or **Folders**) belongs to a user, or to
a master folder, not to a device. Any device of that user attaches a local folder to it, and changes reach the others.
Sharing it with other users is a matter of who may use it.

| Target | What it is |
|---|---|
| `device` (default) | This device's own space on the master. Up only |
| `master:<folder>` | A master folder, made by an administrator under **Folders** (or by a new folder in `files/`). Everyone can sync it; read-only users only read |
| `<owner>/<name>` | A folder of yours, or one shared with you. Both ways |

```sh
fafnirx folder add ~/Music --target master:Music           # a master folder
fafnirx folder add ~/Camera --target ana/photos --mode up  # a folder of yours, send only
fafnirx folder add ~/Family --target ana/family --create   # make the folder first
fafnirx folder list [--available]            # --available: what the master offers
fafnirx folder mode ana/family up|both|keep|ask
fafnirx folder remove ana/family             # stops syncing here; keeps the files
fafnirx folder keep ana/family               # keep files whose deletion is held back
fafnirx folder resolve ana/family theirs     # answer files waiting under "ask"
```

The tray does the same from its **Folders** menu. A folder is named `<owner>/<name>` (or `master:<folder>`) everywhere; the short
name works when it is unambiguous. Files the client creates in an attached folder belong to the folder's owner.

- **Cycle.** Receive, then send. The master tells running clients about changes (within seconds); a poll every minute is the
  fallback. Unchanged on both sides: untouched; changed on one: copied.
- **Names.** Rules of the most limited device: paths differing only in case, and names Windows cannot store (`: ? * " < > |`,
  trailing dot or space, `CON`, `NUL`...), are refused and reported to the sender.
- **Mode** says which way a folder goes and who wins when a file changed here and on the master (set in the tray's folder menu
  or `folder mode`; per folder and device):
  - `both` (default): both ways; the master's version wins and yours goes to `.fafnirx-conflicts/` (never synced).
  - `up`: send only; nothing of the master's is received or deleted here.
  - `keep`: both ways; yours is kept as `name (conflict <device> <date>).ext` next to the master's.
  - `ask`: needs a screen. The tray shows a dialog with both versions; closing it decides later. After `ask_timeout_hours`
    (default 24) both are kept.
- **Deletes.** Web app deletes apply on every device; the Trash keeps a version. Deletes by other devices pass the delete
  guard; a held one waits until `release-deletes` or `folder keep`, and raises **Deletions held back**.
- **Status.** `fafnirx status` shows each folder (in step or not, last sync, held deletions, last problem), also with the client
  stopped. The **Devices** page shows the same per device, and the command to attach it.
- **Access.** The owner adds users (read-only or read and change) and removes them; a member can leave. A device attached to a folder its user can only read receives it and sends nothing (the tray and `folder list` mark it read-only). Removing a member ends
  their access at once; their local files stay.
- **Deleting a folder.** Delete its files first; then the folder, its trash and history go. Attached devices keep their files;
  run `folder remove` there.

**macOS** needs Full Disk Access for folders in Documents, Desktop, Downloads, iCloud Drive and other volumes. Without it
`fafnirx status` says `ATTENTION` and the tray offers to open the settings; then `sudo fafnirx restart`.

## Settings and retention

| Setting | Default | Meaning |
|---|---|---|
| Versions kept per file | 5 | Older versions expire, then go after the retention period |
| Retention (days) | 30 | How long deleted files and expired versions stay |
| Thumbnails and previews | on | Off: none is made, even on demand |
| Photo pages | on | Off: Timeline, Favorites, Albums and Archive are hidden for everybody; files are still processed and shown in Files |

A large file that changes often takes about *versions kept* times its size.

## Local folders and the network drive

`data/files/` is a plain tree: `files/users/<user>/<name>/<path>` (`files/users/<user>/<device>/<directory>/<path>` for what a device sends), master folders in `files/<name>/<path>`. Share it over SMB/NFS
([Deployment](deployment.md#storage)).

- Write as `FAFNIRX_UID`. A new `files/users/<user>/<name>` becomes a folder of that user; a new folder directly under
  `files/` is a master folder.
- Only folders of users and master folders are written in place. Share device and source folders read-only: writes there are not recorded.
- Changes are noticed within seconds, or every 5 minutes on mounts without events. Administrators can press **Scan now**
  (Files, Settings → File store).
- A moved file is recorded as a `rename` if it keeps its size and mtime; otherwise as `delete` and `add`.

## Alerts

| Alert | Raised when | Clears |
|---|---|---|
| Device offline | No heartbeat for 60 s | Seen again |
| Upload error | A client reports one | 48 h after the last report |
| Disk usage | Data disk over 80% | Usage drops |
| Source / Mirror failed | A run fails or is partial | A good run |
| Device used by two devices | A second device uses the token | It stops |
| Deletions held back | A device holds a large deletion | Dealt with, or no report for 15 min |
| Files outside a folder | Files in `files/users/` not under `<user>/<name>/` | Moved or removed |

Dismiss by hand. Collection never pauses automatically. There is no email or webhook.

## Restoring data

In the web app: open the file, pick a version, **Download** or **Restore**. On disk: the current version is
`<data>/files/users/<user>/<device>/<path>`; replaced or deleted versions are in `<data>/versions/<version id>/...`. Or copy from
a replica, or restore a backup with its restore kit ([Mirrors](mirrors.md#restore)).

## Client updates

The client checks GitHub releases every 6 hours; when there is a newer one it downloads it, checks the `.sha256` (and its signature, when the release is
signed), replaces its
own file and restarts. Config, token and state stay, so nothing is sent again. `auto_update = false` turns this off.

- **Tray:** **Check for updates** looks; when something newer exists the entry becomes **Update to `<version>`**, which installs
  it and restarts the client and the tray.
- **CLI:** `fafnirx update --check` prints `available <version>` or `current <version>`; `fafnirx update` installs and waits
  until the client runs again. `fafnirx restart` restarts the client and the tray.
- A `.deb` install (under `/usr`) is updated through `dpkg` (`sudo` or `pkexec` asks); the background check only reports.
- An install stops and removes older copies of `fafnirx` in the usual places and on the `PATH`, so one version stays.

`FAFNIRX_UPDATE_URL` replaces the address of the releases API (a fork, or a test server).

Signing is optional. With the repository secret `UPDATE_SIGNING_KEY` the Release workflow signs each checksum (`.sha256.sig`,
Ed25519), and a client installs a signed release only if the signature is by its key (`UPDATE_KEY` in
`crates/client/src/update/release.rs`; a fork puts its own there). An unsigned release is checked by checksum alone, which
guards against bad downloads, not against someone who can publish releases.
