# Plan: the Android app

Status: **planned, not started.** Nothing in this document is built. It records what the phone app would need from
the master.

## Goal

Replace the part of [Immich](https://immich.app) that people use most: **back up the photos and videos of a phone to the
server you own, automatically, and look at them from the phone.** The master already stores, versions, mirrors and
serves files; the phone becomes one more device that sends, plus a viewer.

Non-goals for the first version of the app: face recognition, object or text search by machine learning, partner
sharing, shared libraries, and (in the app itself) albums and a map. The web app already has albums
and a map; the app can show them later. None of this is needed to be safe or useful.

## The first version

1. **Back up** new photos and videos from chosen device folders (Camera, Screenshots, WhatsApp, ...) automatically.
2. **Never lose or alter an original**: upload the file exactly as it is, with its EXIF data and modification time.
3. **Never delete anything on the phone**, and nothing the phone does can delete from the master (the same one-way rule
   as every device).
4. **Resumable and polite**: only on Wi-Fi (or any network, per setting), optionally only while charging; survives
   being killed, a network change and a reboot.
5. **See the backup**: a timeline of what the master holds for this phone, newest first, grouped by day, with thumbnails,
   a full-screen viewer, and download.
6. **Set up in a minute**: scan or paste the address and token the web app shows when a device is added.

## How it fits what exists

| Need | Already there | To add |
|---|---|---|
| Upload protocol | gRPC `Negotiate` / `UploadChunks` / `Commit`: chunked, resumable, verified, deduplicated by content hash | Nothing. The phone is a normal device (`kind = device`) with a token |
| Identity | Device token, rotate and revoke in the web app | Show the enroll details as a QR code in the web app |
| Transport | Plain HTTP on a LAN or VPN, `https://` behind a Cloudflare tunnel | Android blocks cleartext traffic by default: allow it per configured host (a network security config), and prefer https for anything public |
| Photo metadata | `file_meta` (dimensions, EXIF, date taken, GPS, video length and codec), read by a worker on every new photo and video | HEIC (the preview decoder does not read HEIC; ffmpeg only if its build can) |
| Previews | `GET /api/media/:id/{thumb,preview,video}` (JPEG pictures, video poster frames and playable copies, cached by content) | HEIC |
| Reading the backup | Admin API: browse, search, download with ranges, restore; the **timeline** (`/api/timeline/buckets` and `/api/timeline`), albums, map and trash the web app uses | A read-only **API token** (below), so the phone does not need the admin login |
| Remote access | Cloudflare tunnel | Cloudflare Access service tokens for the app, if the web app is behind Access |

### Server work needed (later)

- **Scoped API tokens.** Today the admin API has one login (a cookie from the admin password). The app needs a token that
  can only *read* what belongs to its own device: list, thumbnails, downloads. Tokens are created in the web app, shown
  once, stored hashed, revocable, and carried as `Authorization: Bearer`. Writing and deleting stay impossible with it.
- ~~**Timeline endpoint**~~ Built for the web app: `GET /api/timeline/buckets` (months with the file count of each day)
  and `GET /api/timeline?month=YYYY-MM` (with `device`, `kind`, `tag` and `scope` filters), returning each file with the
  fields the grid needs. The phone can use them as they are once it has a token; it will want `device=<its name>`.
- ~~**Metadata for videos** and a poster frame~~ Built (`ffprobe`/`ffmpeg`, see the [usage guide](../usage.md#photos)).
- **HEIC and AVIF previews.** Phones increasingly write HEIC. Decoding needs `libheif`; until then the app can ask the
  phone to convert on upload, or the master can serve the original and let the phone's own decoder show it.
- **Motion photos and Live Photos**: they are two files (or one file with an appended video). Store both unchanged and
  link them by name in the viewer.

## The app

### Structure

A native Kotlin app (Jetpack Compose) with three parts:

1. **Sync engine**: finds new media, hashes it in 4 MiB chunks with BLAKE3, and runs the upload protocol.
2. **Settings and enrollment**: server address, token, folders, network and charging rules, storage of the token in the
   Android Keystore.
3. **Timeline viewer**: the grid and the full-screen viewer, reading through the admin API with the read-only token.

### Sharing the protocol with the other clients

The chunking, hashing and manifest rules live in `crates/common` and the sync logic in `crates/client`. Two ways to
use them on Android; decide at the start of the work:

- **A Rust core through UniFFI** (a library of the existing hashing, chunking and upload code, called from Kotlin).
  One implementation of the protocol for every platform, so no drift. Cost: building the Rust library for the Android ABIs
  and a thin Kotlin wrapper. **Preferred.**
- **A Kotlin implementation** of the protocol with `grpc-kotlin` and a BLAKE3 library. No native build, but the protocol
  exists twice and must be kept equal by tests that run both against one master.

Either way, `proto/fafnirx.proto` is the contract and the end-to-end suite in `tests/` is the check.

### Finding new media

- Observe `MediaStore` (`ContentObserver`) for images and videos, and also run a periodic full query as a backstop
  (observers are not delivered while the app is not running).
- Remember what was uploaded in a local database (the content URI, size, date modified, and the root hash once known), as
  the desktop client does with its cursor.
- Only the folders (buckets) the user picked; default to the camera.
- Read the original bytes through the content resolver; on Android 10 and later with scoped storage, request
  `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO` and, for the original GPS data, `ACCESS_MEDIA_LOCATION`.

### Background work

- `WorkManager` with constraints (unmetered network, charging optional, battery not low) for the regular backup.
- A foreground service with a notification while a large backup runs, so that the system does not stop it.
- Chunk-level resume (the server tells which chunks it has), so a killed upload continues, and a changed network address
  just retries.
- A "back up now" button, and a clear state on screen: waiting for Wi-Fi, uploading 120 of 3,400, up to date.

### What the user sees

- A progress screen, a timeline, a viewer with the usual gestures and an info panel (date, camera, size, where it is
  stored), and a download button.
- Plain wording, no accounts, no cloud: the server address and a name for the phone are all that is asked.

## Phases

| Phase | Result |
|---|---|
| 0. Decide | Rust core or Kotlin; minimum Android version (8.0, API 26, is a sensible floor); how the app is distributed (a signed APK from the web app, F-Droid, or both) |
| 1. Server groundwork | API tokens, the timeline endpoint, video metadata and posters, HEIC previews, the QR code on the device page |
| 2. Backup only | Enrollment, folder choice, background upload with resume, status screen. A phone can back up and the web app shows its files |
| 3. Timeline | The grid, the viewer, download |
| 4. Polish | Battery and network rules, notifications, error reporting through the existing upload-error alert, localization |
| 5. Later | Albums, a map from EXIF GPS, search by date and place, sharing, face and object recognition as an optional background job |

## Risks and open questions

- **Background limits differ by manufacturer.** Some stop apps aggressively. Mitigation: a foreground service, the
  standard battery-optimization exemption prompt, and a visible "last backed up" time so that a stalled backup is noticed.
- **Cleartext HTTP.** Android blocks it unless configured. The plan is a network security config that allows cleartext
  only for the host the user entered, with a warning for public addresses, in line with the desktop client.
- **Large videos** (several GB) through a Cloudflare tunnel: gRPC streams work, but a tunnel can drop a slow connection;
  chunked resume covers it. Test on a real mobile network.
- **Storage on the phone is not freed** by the app. A "free up space" feature (deleting local copies after verifying them
  on the master) is deliberately left out: it would need the app to read from the master and to delete locally, and the
  one-way rule is worth more than the convenience.
- **Edited photos** produce a new file (a new content hash), so the original and the edit are both kept; the phone's own
  gallery decides which it shows.
- **Duplicates across devices** are not merged (each device is its own folder). A later "same content" view could use
  the content hash.
- Open: should the timeline show the files of every device or only the phone's own? Start with the phone's own, and let
  the web app remain the place to see everything.
