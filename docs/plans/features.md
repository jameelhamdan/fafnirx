# Plans

Nothing here is built; the rest of the documentation describes only what exists.

## Features

- **Edit files in local folders** from the web app (move and rename are built; content editing is not). Files a computer or a source sent stay unmovable by design.
- **Android client** that backs up a phone's photos: [plan](android.md). The app itself lives in [fafnirx-android](https://github.com/jameelhamdan/fafnirx-android).
- **Scoped API tokens** for the web API (needed by the phone app).
- **Google Keep source**, through an unofficial library.
- **Email and webhook alerts.**
- **Embedded cover art** of videos (MKV attachments, MP4 `attached_pic`) as the picture of a video.

## Release work

- Screenshots in the README.
- Try the Homebrew workflow; publish the tap and, if wanted, an apt repository (releases already carry `.deb` files).
- Test Linux/macOS autostart, the tray, Google Drive against Google, the Cloudflare setup and the video parts against a real ffmpeg.
- A changelog, a code of conduct, issue templates.
- A note about the OpenStreetMap tile usage policy for the default map tiles.
