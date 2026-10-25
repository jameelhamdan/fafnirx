# player

A React video player with its own controls, adaptive streaming (HLS) and subtitles the viewer can choose and restyle.
Nothing in this folder imports from the app around it.

```tsx
import { VideoPlayer } from './player'

<VideoPlayer
  source={{
    src: '/files/holiday.mp4',                   // the file as it is: always the way out
    title: 'Holiday',
    poster: '/files/holiday.jpg',
    hls: '/stream/holiday/master.m3u8',          // optional: adaptive stream
    subtitles: {                                 // optional
      list: '/stream/holiday/subtitles.json',
      track: '/stream/holiday/subtitles/{index}.vtt',
    },
  }}
  onFail={() => showTheDownloadLink()}
/>
```

Needs React 18/19, [hls.js](https://github.com/video-dev/hls.js), [lucide-react](https://lucide.dev), Tailwind CSS 4 with a
`primary` color (the seek bar; classes are in the markup, there is no stylesheet), and a browser with `fetch`, text tracks,
`ResizeObserver` (`localStorage` is optional).

## Props

| Prop | |
|---|---|
| `source` | `src`, `title`, optional `poster`, `hls`, `subtitles`. Use `key={source.src}` to start over for another video |
| `onFail` | The file itself cannot be played |
| `onNotice` | A short message for the viewer (e.g. streaming failed, playing the original) |
| `t` | Translates the English interface texts: `(text) => translated` |
| `lang` | Language of the page, to pick fitting subtitles |
| `storage` | Prefix for what is remembered in the browser: `<storage>.subtitles`, `<storage>.subtitle-style` (default `player`) |
| `globalKeys` | Keys work anywhere on the page (for a full-page viewer); otherwise only while focus is in the player |
| `className` | Added to the outer element |

Keys: space/`k` play, `j`/`l` skip 10 s, `m` mute, `c` subtitles. Arrows are left to the host (previous/next file).

## Addresses

- **`hls`**: an HLS master playlist. The player picks quality by connection (or by hand) and falls back to `src` on failure.
- **`subtitles.list`**: a JSON array `[{ "index": 0, "name": "English", "language": "en", "forced": false }]`.
  **`track`**: the WebVTT of one track, with `{index}` replaced. A `forced` track in the page's language starts by itself;
  otherwise the last choice is kept. Without `subtitles`, the viewer can still load a `.srt`/`.vtt` from their computer.

fafnirx's backend for both is `fafnirx-stream` (`crates/stream`), reached through the master.

## Subtitles

The chosen track is loaded as a hidden text track (the browser parses and times the cues) and drawn by the player, so the
viewer can change the track (or off) or load a file; size, text color, background and opacity, edge (none, outline, shadow),
font (sans, serif, mono); height above the bottom; and a delay in tenths of a second (the cues are moved, so it holds across
seeking). The look is kept in the browser, the delay is per video. Only italic, bold and underline are drawn; nothing in a
subtitle file is taken as HTML.

## Files

| | |
|---|---|
| `video-player.tsx` | The player: controls, keys, full screen |
| `use-source.ts` | What plays: stream or file, quality, switching without losing the place |
| `use-subtitles.ts` | Tracks, choice, cues, delay, local files |
| `settings-menu.tsx`, `menu.tsx` | The gear menu and its parts |
| `subtitle-overlay.tsx`, `subtitle-style.ts`, `cue-text.tsx`, `subtitle-file.ts` | Drawing and styling cues; reading `.srt` |
| `types.ts`, `util.ts`, `index.ts` | Contract, helpers, entry |
