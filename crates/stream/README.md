# fafnirx-stream

Adaptive video streaming as a small service on its own: it turns a video file on disk into HLS playlists and segments
(made by `ffmpeg` when they are asked for, kept in a cache), and lists and converts the file's subtitles to WebVTT. It knows
nothing about users, albums or databases. Whoever runs it puts a login in front, as fafnirx's master does.

Pair it with the `player` module of the web app (`frontend/src/player`), which speaks exactly this API.

## Running it

```sh
FAFNIRX_STREAM_TOKEN=<at least 16 characters> FAFNIRX_DATA=/data fafnirx-stream
```

Videos are read from `<FAFNIRX_DATA>/files/<path>`; the cache is `FAFNIRX_STREAM_CACHE`. It needs `ffmpeg` and `ffprobe`.
All settings are listed in [docs/streaming.md](../../docs/streaming.md).

## The API

Every request has `Authorization: Bearer <token>` and names the file by query:

- `p`: where the file is below `files/` (`alice/laptop/trip/clip.mp4`, or `clip.mp4` for a file at the root)
- `h`: hex hash of its content. A changed file has another `h`, so it gets new cache entries and never a stale segment.

| Request | Answer |
|---|---|
| `GET /hls/master.m3u8` | The playlist of qualities, up to the video's own size |
| `GET /hls/<quality>/index.m3u8` | The segments of one quality, for example `720p` |
| `GET /hls/<quality>/seg<n>.ts` | One segment, made when asked for |
| `GET /hls/subtitles.json` | `[{ "index": 0, "name": "English", "language": "en", "forced": false }]` |
| `GET /hls/subtitles/<index>.vtt` | One track as WebVTT |
| `GET /health` | `200` without the token |

A request the service cannot answer says why in the body (not a video, `ffmpeg` failed); it never includes the token or paths.

## Subtitles

The list is, in this order: the text tracks inside the file (SubRip, ASS/SSA, WebVTT, `mov_text`; picture tracks such as PGS
cannot be converted and are left out), then the subtitle files next to the video (`Movie.srt`, `Movie.en.srt`,
`Movie.es.forced.srt` for `Movie.mp4`; also in a `Subs` folder next to it; a video that is alone in its folder gets every
subtitle file there). Each is converted once and kept. The cues carry the video's own times, so they stay in step whichever
quality plays.

## Code

| | |
|---|---|
| `hls.rs` | Playlists, the quality ladder, segment arithmetic, subtitle file naming. Pure, no I/O. |
| `ffmpeg.rs` | Running `ffprobe` and `ffmpeg`: reading a video, cutting a segment, converting subtitles. |
| `service.rs` | The HTTP side: auth, locating a file, playlists, segments, the cache sweep. |
| `subtitles.rs` | The subtitle list and tracks (files and embedded). |
| `main.rs` | Settings from the environment. |

What `ffprobe` says about a video is read by `fafnirx_common::probe`, which the master shares for its own video details.
