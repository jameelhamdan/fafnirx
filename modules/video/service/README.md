# fafnirx-video

The video [module](../../../docs/modules.md) of fafnirx, as a small service of its own: it turns a video file on disk into HLS
playlists and segments (made by `ffmpeg` when asked for, then cached), lists and converts subtitles to WebVTT, and tells the
master what a video is and draws a frame of it. It knows nothing of users, albums or databases; whoever runs it puts a login in
front, as fafnirx's master does. Pair it with the web app's video module (`frontend/src/modules/video`), which speaks exactly this
API; the player inside it (`player/`) imports nothing from the app.

```sh
FAFNIRX_MODULE_TOKEN=<16+ characters> FAFNIRX_DATA=/data fafnirx-video
```

Videos are read from `<FAFNIRX_DATA>/files/<path>`; the cache is `FAFNIRX_VIDEO_CACHE`. Needs `ffmpeg` and `ffprobe`.
All settings: [docs/video.md](../../../docs/video.md). The contract it implements: `crates/module-api`.

## API

Every request has `Authorization: Bearer <token>` and names the file by query:

- `p`: the file's path below `files/` (`users/alice/laptop/trip/clip.mp4`, or `clip.mp4` at the root)
- `h`: hex hash of its content. A changed file has another `h`, so it gets new cache entries and never a stale segment.

What the master asks for:

| Request | Answer |
|---|---|
| `GET /manifest` | `{ "id": "video", "version", "contract", "capabilities": ["probe.video", "poster", "status"], "ui": { "entry", "css", "sdk" } }` (`ui` once the web part is built) |
| `GET /probe` | `{ duration_ms, width, height, codec, taken_at, position }`; `415` when it is not a video |
| `GET /poster?max=320` | A JPEG frame no larger than `max` px (16 to 4096); `422` when no frame could be taken |
| `GET /status` | What is being made (`segment`, `subtitles`; waiting or running), the last failures, how many run at once. In memory: a restart clears it |
| `GET /ui/<file>` | A file of the web part (`index.js`, its chunks, `style.css`) from `FAFNIRX_MODULE_UI`; the master passes it on to the browser |
| `GET /health` | `200` without the token |

What the web app asks for, through the master (`GET /api/m/video/<file id>/<rest>` is `GET /files/<rest>`):

| Request | Answer |
|---|---|
| `GET /files/hls/master.m3u8` | Playlist of qualities, up to the video's own size |
| `GET /files/hls/<quality>/index.m3u8` | Segments of one quality (`720p`) |
| `GET /files/hls/<quality>/seg<n>.ts` | One segment, made when asked for |
| `GET /files/hls/subtitles.json` | `[{ "index": 0, "name": "English", "language": "en", "forced": false }]` |
| `GET /files/hls/subtitles/<index>.vtt` | One track as WebVTT |

Errors say why in the body (not a video, `ffmpeg` failed) and never include the token or paths.

## Subtitles

In order: text tracks inside the file (SubRip, ASS/SSA, WebVTT, `mov_text`; picture tracks such as PGS are left out), then
files next to the video (`Movie.srt`, `Movie.en.srt`, `Movie.es.forced.srt` for `Movie.mp4`; also in a `Subs` folder; a video
alone in its folder takes every subtitle file there). Each is converted once and kept. Cues carry the video's own times, so
they fit any quality.

## Code

| | |
|---|---|
| `hls.rs` | Playlists, quality ladder, segment arithmetic, subtitle file naming. Pure |
| `ffmpeg.rs` | Running `ffprobe` and `ffmpeg`: probing, cutting segments, taking a frame, converting subtitles |
| `probe.rs` | Reading `ffprobe`'s JSON: which stream is the picture, how a sideways video is measured, which length counts |
| `derive.rs` | The manifest, and what the master asks for: details (`/probe`) and a frame (`/poster`) |
| `service.rs` | HTTP: routes, auth, locating a file, playlists, segments, cache sweep |
| `subtitles.rs` | The subtitle list and tracks |
| `tasks.rs` | The list behind `GET /status` |
| `ui.rs` | The web part's files and what the manifest says of them |
| `main.rs` | Settings from the environment |
