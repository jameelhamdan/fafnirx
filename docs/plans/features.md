# Plans: features and release work

Nothing here exists yet. The rest of the documentation describes only what is built.

## Features

- **Rename, move and edit in local folders** from the web app. Files in local folders (the ones uploaded to the master)
  can be uploaded and deleted today, but not renamed or moved.
- **Android client** that backs up the photos of a phone: see [the Android plan](android.md).
- **Scoped API tokens** for the web API, which the phone app needs.
- **Google Keep source**, through an unofficial library.
- **Email and webhook alerts.**

## Release work

- Screenshots in the README.
- Try the release, Docker and Homebrew workflows on a pre-release before announcing.
- Test the Linux and macOS autostart, the tray icon, Google Drive against Google, the Cloudflare tunnel setup and the
  video parts against a real ffmpeg.
- Publish the Homebrew tap and, if wanted, an apt repository (releases already carry `.deb` files).
- A changelog, a code of conduct and issue templates.
- A note in the deployment guide about the OpenStreetMap tile usage policy for the map's default tile server.
