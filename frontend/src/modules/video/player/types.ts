/** Where the subtitle tracks of a video are. */
export interface SubtitleSource {
  /** Address of a JSON array of {@link SubtitleTrack}. */
  list: string
  /** Address of the WebVTT of one track, with `{index}` where the track's number goes (`/video/subtitles/{index}.vtt`). */
  track: string
}

/** One subtitle track, as the list answers. */
export interface SubtitleTrack {
  index: number
  /** What to call it: the language in its own script, a title, or "Subtitles". */
  name: string
  /** Two-letter code, empty when not known. */
  language: string
  /** Only for speech in another language than the rest: the player starts these by itself. */
  forced: boolean
}

/** Everything the player needs to know about one video. It reads nothing else from the app around it. */
export interface PlayerSource {
  /** The file as it is. Plays wherever the browser knows its format, and is the way out when streaming fails. */
  src: string
  poster?: string
  /** Name of the video, for screen readers. */
  title: string
  /** The adaptive stream (an HLS master playlist). Without it the file plays as it is. */
  hls?: string
  subtitles?: SubtitleSource
}

/** Interface texts. They are English; the app around the player may translate them. */
export type Translate = (text: string) => string

export interface PlayerProps {
  source: PlayerSource
  /** The video cannot be played at all (the file itself fails). */
  onFail: () => void
  /** A short message for the person watching, for example that streaming failed and the original plays. */
  onNotice?: (message: string) => void
  /** Translates the interface texts. */
  t?: Translate
  /** Language of the page: a forced subtitle track in that language starts by itself. */
  lang?: string
  /** Prefix of what the player remembers in the browser: the subtitle choice and look (default "player"). */
  storage?: string
  /** Space, arrows and letters work anywhere on the page (for a viewer that fills it); otherwise only with focus inside. */
  globalKeys?: boolean
  className?: string
}
