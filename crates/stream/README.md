# fafnirx-stream

Adaptive video streaming as a small service of its own: it turns a video file on disk into HLS playlists and segments (made by
`ffmpeg` when asked for, then cached) and lists and converts subtitles to WebVTT. It knows nothing of users, albums or
databases; whoever runs it puts a login in front, as fafnirx's master does. Pair it with the web app's `player` module
(`frontend/src/player`), which speaks exactly this API.

```sh
FAFNIRX_STREAM_TOKEN=<16+ characters> FAFNIRX_DATA=/data fafnirx-stream
```

Videos are read from `<FAFNIRX_DATA>/files/<path>`; the cache is `FAFNIRX_STREAM_CACHE`. Needs `ffmpeg` and `ffprobe`.
All settings: [docs/streaming.md](../../docs/streaming.md).

## API

Every request has `Authorization: Bearer <token>` and names the file by query:

- `p`: the file's path below `files/` (`alice/laptop/trip/clip.mp4`, or `clip.mp4` at the root)
- `h`: hex hash of its content. A changed file has another `h`, so it gets new cache entries and never a stale segment.

| Request | Answer |
|---|---|
| `GET /hls/master.m3u8` | Playlist of qualities, up to the video's own size |
| `GET /hls/<quality>/index.m3u8` | Segments of one quality (`720p`) |
| `GET /hls/<quality>/seg<n>.ts` | One segment, made when asked for |
| `GET /hls/subtitles.json` | `[{ "index": 0, "name": "English", "language": "en", "forced": false }]` |
| `GET /hls/subtitles/<index>.vtt` | One track as WebVTT |
| `GET /tasks` | What is being made (`segment`, `subtitles`; waiting or running), the last failures, how many run at once. In memory: a restart clears it |
| `GET /health` | `200` without the token |

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
| `ffmpeg.rs` | Running `ffprobe` and `ffmpeg`: probing, cutting segments, converting subtitles |
| `service.rs` | HTTP: auth, locating a file, playlists, segments, cache sweep |
| `subtitles.rs` | The subtitle list and tracks |
| `tasks.rs` | The list behind `GET /tasks` |
| `main.rs` | Settings from the environment |

`ffprobe` output is read by `fafnirx_common::probe`, shared with the master.
