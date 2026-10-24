# player

A video player for React with its own controls, adaptive streaming (HLS), and subtitles the viewer can choose and
restyle. It is written to be lifted out of fafnirx: nothing in this folder imports from the app around it.

```tsx
import { VideoPlayer } from './player'

<VideoPlayer
  source={{
    src: '/files/holiday.mp4',                   // the file as it is: always the way out
    title: 'Holiday',
    poster: '/files/holiday.jpg',
    hls: '/stream/holiday/master.m3u8',          // optional: the adaptive stream
    subtitles: {                                 // optional: subtitle tracks
      list: '/stream/holiday/subtitles.json',
      track: '/stream/holiday/subtitles/{index}.vtt',
    },
  }}
  onFail={() => showTheDownloadLink()}
/>
```

## What it needs

- React 18 or 19, [hls.js](https://github.com/video-dev/hls.js), [lucide-react](https://lucide.dev) for the icons.
- Tailwind CSS 4, with a `primary` color (the seek bar). The classes are in the markup; there is no stylesheet.
- A browser. It uses `fetch`, text tracks, `ResizeObserver` and `localStorage` (and works without the last).

## Props

| Prop | |
|---|---|
| `source` | `src`, `title`, and optionally `poster`, `hls`, `subtitles` (see below). Use `key={source.src}` to start over for another video. |
| `onFail` | The file itself cannot be played. |
| `onNotice` | A short message for the viewer, for example "Streaming did not work; playing the original". |
| `t` | Translates the interface texts, which are English: `(text) => translated`. Default: as they are. |
| `lang` | Language of the page, for choosing the subtitles that fit it. |
| `storage` | Prefix of what is remembered in the browser: `<storage>.subtitles` (the last choice) and `<storage>.subtitle-style`. Default `player`. |
| `globalKeys` | Keys work anywhere on the page (for a viewer that fills it). Otherwise only while focus is in the player. |
| `className` | Added to the outer element. |

The keys: space or `k` play/pause, `j` and `l` skip 10 s, `m` mute, `c` subtitles on/off. The arrows are left to whoever
shows the player (a viewer goes to the previous and next file with them).

## The two addresses

**`hls`** is an HLS master playlist. The player picks the quality by the connection (or by hand) and falls back to `src`
if the stream fails.

**`subtitles.list`** answers a JSON array:

```json
[{ "index": 0, "name": "English", "language": "en", "forced": false }]
```

`track` is the address of the WebVTT of one track, with `{index}` where the track's `index` goes. A `forced` track in
the language of the page starts by itself; otherwise the last choice is kept. Without `subtitles`, the viewer can still
load a `.srt` or `.vtt` file from their computer.

fafnirx's own backend for both is the `fafnirx-stream` service (`crates/stream`, see its README), reached through the master.

## Subtitles

The player does not use the browser's own subtitle drawing. The chosen track is loaded as a hidden text track (so the
browser parses and times the cues) and drawn by the player, so the viewer can change:

- the track (or off), or load a file;
- size, text color, background and its opacity, edge (none, outline, shadow), font (sans, serif, mono);
- the height above the bottom;
- a delay in tenths of a second, for a file that is out of step. The cues themselves are moved, so it holds across seeking.

The look is kept in the browser, the delay is for this video only. Only italic, bold and underline of a cue's markup are
drawn; nothing in a subtitle file is ever taken as HTML.

## Files

| | |
|---|---|
| `video-player.tsx` | The player: controls, keys, full screen. |
| `use-source.ts` | What plays: stream or file, quality, switching without losing the place. |
| `use-subtitles.ts` | Tracks, choice, cues, delay, local files. |
| `settings-menu.tsx`, `menu.tsx` | The gear menu and its parts. |
| `subtitle-overlay.tsx`, `subtitle-style.ts`, `cue-text.tsx`, `subtitle-file.ts` | Drawing and styling the cues; reading a `.srt`. |
| `types.ts`, `util.ts`, `index.ts` | The contract, small helpers, the entry. |
