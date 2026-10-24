import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { MAX_SUBTITLE_BYTES, toVtt } from './subtitle-file'
import { loadStyle, saveStyle, DEFAULT_STYLE, type SubtitleStyle } from './subtitle-style'
import type { PlayerSource, SubtitleTrack, Translate } from './types'
import { recall, remember } from './util'

/** A track the player can show: one the source lists, or a file the person picked. */
export interface Track extends SubtitleTrack {
  url: string
  local: boolean
}

/** Numbers of the tracks from files, so that they never meet the numbers of the listed ones. */
const LOCAL_BASE = 10_000

/** What the choice is remembered by: the language when the track has one, else its name. */
const kindOf = (track: SubtitleTrack) => track.language || track.name

function isTrack(value: unknown): value is SubtitleTrack {
  const o = value as Record<string, unknown> | null
  return !!o && typeof o.index === 'number' && typeof o.name === 'string' && typeof o.language === 'string' && typeof o.forced === 'boolean'
}

/** The track to start with, by its number (-1: none): what was chosen last time; else a forced track (foreign speech only) in the language of the page. */
function firstChoice(list: SubtitleTrack[], wanted: string | null, lang: string | undefined) {
  if (wanted === 'off') return -1
  const found = wanted ? list.find((s) => kindOf(s) === wanted) : list.find((s) => s.forced && s.language === lang)
  return found?.index ?? -1
}

const sameLines = (a: string[], b: string[]) => a.length === b.length && a.every((line, i) => line === b[i])

interface Options {
  video: HTMLVideoElement | null
  source: PlayerSource
  lang?: string
  storage: string
  t: Translate
  onNotice?: (message: string) => void
}

/**
 * The subtitles of a video: which tracks there are, which one is shown, how it looks and how far it is moved in time.
 * The browser loads and times the cues of the chosen track (as a hidden text track), and the player draws them itself,
 * so the look and the timing are the person's to change. A new video is a new player (the caller keys it by the
 * video), so nothing here has to be reset.
 */
export function useSubtitles({ video, source, lang, storage, t, onNotice }: Options) {
  const [remote, setRemote] = useState<SubtitleTrack[]>([])
  const [local, setLocal] = useState<Track[]>([])
  // The number of the track shown (`SubtitleTrack.index`, not its place in the list, which changes when a file is added), or -1.
  const [selected, setSelected] = useState(-1)
  const [lines, setLines] = useState<string[]>([])
  const [style, setStyleState] = useState<SubtitleStyle>(() => loadStyle(storage))
  const [delay, setDelay] = useState(0)

  const listUrl = source.subtitles?.list
  const template = source.subtitles?.track
  const langRef = useRef(lang)
  useEffect(() => {
    langRef.current = lang
  })
  const delayRef = useRef(0)
  const active = useRef<TextTrack | undefined>(undefined)
  const moved = useRef(new WeakMap<TextTrack, number>())
  const blobs = useRef<string[]>([])
  const choiceKey = `${storage}.subtitles`

  const tracks = useMemo<Track[]>(
    () => [...remote.map((r) => ({ ...r, url: (template ?? '').replace('{index}', String(r.index)), local: false })), ...local],
    [remote, local, template],
  )

  useEffect(() => {
    if (!listUrl) return
    let current = true
    fetch(listUrl, { credentials: 'same-origin' })
      .then((r) => (r.ok ? r.json() : []))
      .then((list: unknown) => {
        if (!current || !Array.isArray(list)) return
        const found = list.filter(isTrack)
        setRemote(found)
        setSelected(firstChoice(found, recall(choiceKey), langRef.current))
      })
      .catch(() => {})
    return () => {
      current = false
    }
  }, [listUrl, choiceKey])
  useEffect(() => {
    const urls = blobs.current
    return () => { for (const url of urls.splice(0)) URL.revokeObjectURL(url) }
  }, [])

  /** The lines of the track that are showing now. */
  const read = useCallback(() => {
    const track = active.current
    if (!video || !track?.cues) return []
    const now = video.currentTime
    return Array.from(track.cues).filter((c) => c.startTime <= now && now < c.endTime).map((c) => (c as VTTCue).text)
  }, [video])
  const refresh = useCallback(() => setLines((before) => { const now = read(); return sameLines(before, now) ? before : now }), [read])

  /** Moves the cues of a track so that they are `by` seconds late in all (what was moved already is remembered). */
  const move = useCallback((track: TextTrack | undefined, by: number) => {
    if (!track?.cues) return
    const step = by - (moved.current.get(track) ?? 0)
    if (step === 0) return
    for (const cue of Array.from(track.cues)) {
      cue.startTime += step
      cue.endTime += step
    }
    moved.current.set(track, by)
  }, [])

  // Which text track of the video is shown: the chosen one is hidden (loaded and timed, but drawn by the player).
  useEffect(() => {
    if (!video) return
    const wanted = tracks.find((s) => s.index === selected)?.name
    const apply = () => {
      active.current?.removeEventListener('cuechange', refresh)
      active.current = undefined
      // Only our own tracks: the stream may add others (timed metadata) of its own.
      for (const track of Array.from(video.textTracks).filter((x) => x.kind === 'subtitles')) {
        if (wanted !== undefined && track.label === wanted) {
          track.mode = 'hidden'
          active.current = track
        } else if (track.mode !== 'disabled') {
          track.mode = 'disabled'
        }
      }
      active.current?.addEventListener('cuechange', refresh)
      move(active.current, delayRef.current)
      refresh()
    }
    // The cues of a track that was not loaded yet are moved when they come (the element says so, not the text track).
    const loaded = (e: Event) => {
      if ((e.target as HTMLTrackElement).track === active.current) {
        move(active.current, delayRef.current)
        refresh()
      }
    }
    const elements = Array.from(video.querySelectorAll('track'))
    for (const element of elements) element.addEventListener('load', loaded)
    apply()
    video.textTracks.addEventListener('addtrack', apply)
    return () => {
      for (const element of elements) element.removeEventListener('load', loaded)
      video.textTracks.removeEventListener('addtrack', apply)
      active.current?.removeEventListener('cuechange', refresh)
      active.current = undefined
      setLines([])
    }
  }, [video, tracks, selected, refresh, move])

  const changeDelay = useCallback((seconds: number) => {
    const next = Math.round(Math.min(30, Math.max(-30, seconds)) * 10) / 10
    delayRef.current = next
    setDelay(next)
    move(active.current, next)
    refresh()
  }, [move, refresh])

  const select = useCallback((index: number) => {
    setSelected(index)
    const track = tracks.find((s) => s.index === index)
    remember(choiceKey, track ? kindOf(track) : 'off')
  }, [tracks, choiceKey])

  /** Turns subtitles off; or on again with the track used last (else one in the language of the page). */
  const toggle = useCallback(() => {
    if (tracks.length === 0) return
    if (selected >= 0) return select(-1)
    const last = tracks.find((s) => kindOf(s) === recall(choiceKey)) ?? tracks.find((s) => s.language === langRef.current)
    select((last ?? tracks[0]).index)
  }, [tracks, selected, select, choiceKey])

  const changeStyle = useCallback((change: Partial<SubtitleStyle>) => {
    setStyleState((before) => {
      const next = { ...before, ...change }
      saveStyle(storage, next)
      return next
    })
  }, [storage])
  const resetStyle = useCallback(() => {
    saveStyle(storage, DEFAULT_STYLE)
    setStyleState(DEFAULT_STYLE)
    changeDelay(0)
  }, [storage, changeDelay])

  /** Adds a subtitle file from the person's computer and shows it. */
  const addFile = useCallback(async (file: File) => {
    try {
      const vtt = file.size <= MAX_SUBTITLE_BYTES ? toVtt(await file.text(), file.name) : null
      if (vtt === null) return onNotice?.(t('Only SubRip (.srt) and WebVTT (.vtt) files can be loaded here'))
      const url = URL.createObjectURL(new Blob([vtt], { type: 'text/vtt' }))
      blobs.current.push(url)
      const base = file.name.replace(/\.[^.]*$/, '') || t('Subtitles')
      let name = base
      for (let n = 2; tracks.some((s) => s.name === name); n++) name = `${base} ${n}`
      const added: Track = { index: LOCAL_BASE + local.length, name, language: '', forced: false, url, local: true }
      setLocal((before) => [...before, added])
      setSelected(added.index)
    } catch {
      onNotice?.(t('That subtitle file could not be read'))
    }
  }, [tracks, local.length, onNotice, t])

  return { tracks, selected, select, toggle, lines, style, changeStyle, resetStyle, delay, changeDelay, addFile }
}

export type Subtitles = ReturnType<typeof useSubtitles>
