# Plan: the Android app

Status: **planned, not started.**

**Goal:** back up a phone's photos and videos to the master automatically and view them there. The phone is one more device
that sends, plus a viewer. Not in version 1: recognition, sharing, albums and a map in the app.

**Version 1**

1. Back up new photos and videos from chosen folders (default Camera).
2. Upload originals unchanged, with EXIF and modification time.
3. Never delete on the phone; nothing the phone does can delete from the master (the one-way rule).
4. Resumable and polite: Wi-Fi only (or any), optionally only while charging; survives kills, network changes, reboots.
5. Timeline of what the master holds for this phone (by day, thumbnails, viewer, download).
6. Setup in a minute: scan or paste the address and token.

| Need | Exists | To add |
|---|---|---|
| Upload | gRPC `Negotiate`/`UploadChunks`/`Commit`: chunked, resumable, verified, deduplicated | Nothing: the phone is a normal device |
| Identity | Device token, rotate, revoke | Enroll details as a QR code in the web app |
| Transport | HTTP on LAN/VPN; https behind a tunnel | Cleartext allowed only for the configured host (network security config) |
| Pictures, metadata | `file_meta`, `/api/media/:id/{thumb,preview,video}` | HEIC/AVIF (needs `libheif`, or convert on upload) |
| Reading the backup | Timeline, albums, map, trash, downloads in the web API | Read-only **API token** (`Authorization: Bearer`, shown once, stored hashed, revocable, limited to its device) |
| Remote access | Cloudflare tunnel | Access service tokens if the web app is behind Access |

Also: motion photos (two files, or one with an appended video) stored unchanged and linked by name.

**App:** native Kotlin (Jetpack Compose): sync engine (find media, hash in 4 MiB BLAKE3 chunks, upload protocol), settings and
enrollment (token in the Keystore), timeline viewer over the web API. Share the protocol either via a **Rust core through
UniFFI** (preferred: one implementation, built for the Android ABIs) or a Kotlin implementation (`grpc-kotlin` + BLAKE3; two
implementations kept equal by tests against one master). `proto/fafnirx.proto` is the contract.

- **Finding media:** `MediaStore` `ContentObserver` plus a periodic full query; a local database of what was uploaded (URI, size,
  modified, root hash); chosen buckets only; originals through the content resolver; permissions `READ_MEDIA_IMAGES`,
  `READ_MEDIA_VIDEO`, `ACCESS_MEDIA_LOCATION`.
- **Background:** `WorkManager` (unmetered, charging optional, battery not low); a foreground service during large backups;
  chunk-level resume; "back up now" and a clear state ("waiting for Wi-Fi", "120 of 3,400").

| Phase | Result |
|---|---|
| 0 Decide | Rust core or Kotlin; minimum Android (8.0 / API 26); distribution (signed APK, F-Droid, or both) |
| 1 Server | API tokens, HEIC previews, QR code on the device page |
| 2 Backup | Enrollment, folders, background upload with resume, status |
| 3 Timeline | Grid, viewer, download |
| 4 Polish | Battery/network rules, notifications, errors via the upload-error alert, localization |
| 5 Later | Albums, map, search, sharing, recognition as an optional job |

**Risks:** manufacturers kill background apps (foreground service, battery prompt, visible "last backed up"); cleartext HTTP
(only for the entered host, warn on public addresses); large videos through a tunnel (chunked resume; test on mobile data);
no "free up space" (it would need local deletes; the one-way rule matters more); edits are new files (both kept); duplicates
across devices are not merged. Open: show every device or only the phone's own in the timeline? Start with the phone's own.
