import Hls from 'hls.js'
import { useCallback, useEffect, useRef, useState } from 'react'
import type { PlayerSource, Translate } from './types'

export const AUTO = -1

export type Mode = 'stream' | 'original'

/**
 * What plays in the video element: the adaptive stream (hls.js, or the browser's own in Safari) when the source has one,
 * else the file as it is, and the file again if the stream fails. Switching keeps the place in the video.
 */
export function useSource(video: HTMLVideoElement | null, source: PlayerSource, t: Translate, onNotice?: (message: string) => void) {
  const { src, hls: playlist } = source
  const player = useRef<Hls | null>(null)
  // Where to carry on, and whether to keep playing, after the source changes under the viewer.
  const resume = useRef({ at: 0, playing: true })
  const [mode, setMode] = useState<Mode>(playlist ? 'stream' : 'original')
  const [heights, setHeights] = useState<number[]>([])
  const [level, setLevel] = useState(AUTO)
  const [shown, setShown] = useState<number | null>(null)

  const keepPlace = useCallback(() => {
    resume.current = { at: video?.currentTime ?? 0, playing: !video?.paused }
  }, [video])

  const toOriginal = useCallback(() => {
    keepPlace()
    setHeights([])
    setLevel(AUTO)
    setMode('original')
  }, [keepPlace])

  const fallBack = useCallback(() => {
    toOriginal()
    onNotice?.(t('Streaming did not work; playing the original'))
  }, [toOriginal, onNotice, t])
  // The effect below must not start over when the caller's functions are new on a render.
  const fallBackRef = useRef(fallBack)
  useEffect(() => {
    fallBackRef.current = fallBack
  })

  useEffect(() => {
    const el = video
    if (!el) return
    const carryOn = () => {
      const { at, playing } = resume.current
      if (at > 0) el.currentTime = at
      if (playing) void el.play().catch(() => {})
      resume.current = { at: 0, playing: true }
    }
    el.addEventListener('loadedmetadata', carryOn, { once: true })
    const stop = () => el.removeEventListener('loadedmetadata', carryOn)
    if (mode === 'stream' && playlist) {
      if (Hls.isSupported()) {
        // No worker: a page may only load its own scripts, and a worker made from a blob is not its own.
        const hls = new Hls({ enableWorker: false, capLevelToPlayerSize: true })
        player.current = hls
        hls.on(Hls.Events.MANIFEST_PARSED, (_e, data) => setHeights(data.levels.map((l) => l.height)))
        hls.on(Hls.Events.LEVEL_SWITCHED, (_e, data) => setShown(data.level))
        hls.on(Hls.Events.ERROR, (_e, data) => {
          if (data.fatal) fallBackRef.current()
        })
        hls.loadSource(playlist)
        hls.attachMedia(el)
        return () => {
          stop()
          hls.destroy()
          player.current = null
        }
      }
      if (el.canPlayType('application/vnd.apple.mpegurl')) {
        // oxlint-disable-next-line react/immutability
        el.src = playlist
        return stop
      }
      fallBackRef.current()
      return stop
    }
    // The element is the outside world this effect keeps in step with the state, so setting it here is the point.
    // oxlint-disable-next-line react/immutability
    el.src = src
    return () => {
      stop()
      el.removeAttribute('src')
      el.load()
    }
  }, [video, mode, src, playlist])

  const pickLevel = useCallback((next: number) => {
    setLevel(next)
    if (player.current) player.current.currentLevel = next
  }, [])

  /** The stream, in automatic quality (from the original: back to the stream, in the same place). */
  const chooseStream = useCallback(() => {
    if (mode === 'stream') return pickLevel(AUTO)
    keepPlace()
    setMode('stream')
  }, [mode, pickLevel, keepPlace])

  return { mode, canStream: !!playlist, heights, level, shown, pickLevel, chooseStream, chooseOriginal: toOriginal }
}

export type SourceState = ReturnType<typeof useSource>
