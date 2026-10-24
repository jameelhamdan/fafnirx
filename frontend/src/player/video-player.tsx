import { Captions, CaptionsOff, Loader2, Maximize, Minimize, Pause, PictureInPicture2, Play, Settings, Volume1, Volume2, VolumeX } from 'lucide-react'
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import { SettingsMenu } from './settings-menu'
import { SubtitleOverlay } from './subtitle-overlay'
import type { PlayerProps, Translate } from './types'
import { AUTO, useSource } from './use-source'
import { useSubtitles } from './use-subtitles'
import { clock, cx } from './util'

/** Controls stay up this long after the pointer last moved, while playing. */
const AWAKE_MS = 2500
const SKIP = 10

const same: Translate = (text) => text

const iconButton =
  'grid size-9 shrink-0 place-items-center rounded-full text-white/90 transition-colors hover:bg-white/15 hover:text-white focus-visible:bg-white/15 focus-visible:outline-none'

/** The parts of a key press the player looks at; both the page's and React's key events have them. */
interface Press {
  key: string
  target: EventTarget | null
  defaultPrevented: boolean
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  preventDefault: () => void
}

/**
 * A video player with its own controls, so that they look the same everywhere, work in full screen and carry the
 * quality and subtitle menus. With an adaptive stream it plays that (several qualities, chosen by the connection or by
 * hand) and falls back to the file as it is when that fails. Subtitles come from the source and from files the person
 * picks, and are drawn by the player in the look the person chose.
 *
 * It needs React, hls.js, lucide-react and Tailwind (with a `primary` color), and nothing from the app around it: see
 * `README.md`.
 */
export default function VideoPlayer({ source, onFail, onNotice, t = same, lang, storage = 'player', globalKeys = false, className }: PlayerProps) {
  const box = useRef<HTMLDivElement>(null)
  // The element twice: in state, so that what depends on it starts when it is there, and in a ref for the handlers that
  // drive it (play, seek, volume).
  const [video, setVideo] = useState<HTMLVideoElement | null>(null)
  const media = useRef<HTMLVideoElement | null>(null)
  const attach = useCallback((el: HTMLVideoElement | null) => {
    media.current = el
    setVideo(el)
  }, [])
  const bar = useRef<HTMLDivElement>(null)
  const controls = useRef<HTMLDivElement>(null)
  const sleep = useRef<number | undefined>(undefined)

  const stream = useSource(video, source, t, onNotice)
  const subs = useSubtitles({ video, source, lang, storage, t, onNotice })
  const toggleCaptions = subs.toggle

  const [playing, setPlaying] = useState(false)
  const [waiting, setWaiting] = useState(true)
  const [time, setTime] = useState(0)
  const [duration, setDuration] = useState(0)
  const [buffered, setBuffered] = useState(0)
  const [volume, setVolume] = useState(1)
  const [muted, setMuted] = useState(false)
  const [rate, setRate] = useState(1)
  const [full, setFull] = useState(false)
  const [menu, setMenu] = useState(false)
  const [awake, setAwake] = useState(true)
  const [hover, setHover] = useState<{ x: number; at: number } | null>(null)
  const [scrubbing, setScrubbing] = useState(false)
  const [height, setHeight] = useState(360)
  // How much of the picture's bottom the controls cover (their content and the padding under it, and a little air).
  const [covered, setCovered] = useState(72)
  const [previewing, setPreviewing] = useState(false)

  useEffect(() => {
    const el = box.current
    if (!el) return
    const watch = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height))
    watch.observe(el)
    const bottom = controls.current
    const watchControls = new ResizeObserver(([entry]) => {
      const padding = parseFloat(getComputedStyle(entry.target).paddingBottom) || 0
      setCovered(Math.ceil(entry.contentRect.height + padding + 8))
    })
    if (bottom) watchControls.observe(bottom)
    return () => {
      watch.disconnect()
      watchControls.disconnect()
    }
  }, [])
  useEffect(() => {
    const onChange = () => setFull(document.fullscreenElement === box.current)
    document.addEventListener('fullscreenchange', onChange)
    return () => document.removeEventListener('fullscreenchange', onChange)
  }, [])
  useEffect(() => () => window.clearTimeout(sleep.current), [])
  // Escape closes the menu and nothing else. A dialog around the player (the file details) listens for it before the page's
  // own handlers, so this listens earlier still, on the window, and the dialog never hears it.
  useEffect(() => {
    if (!menu) return
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.stopPropagation()
      e.preventDefault()
      setMenu(false)
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [menu])

  const wake = useCallback(() => {
    setAwake(true)
    window.clearTimeout(sleep.current)
    sleep.current = window.setTimeout(() => setAwake(false), AWAKE_MS)
  }, [])

  const toggle = useCallback(() => {
    const el = media.current
    if (!el) return
    if (el.paused) void el.play().catch(() => {})
    else el.pause()
  }, [])
  const seekTo = useCallback((seconds: number) => {
    const el = media.current
    if (!el || !Number.isFinite(el.duration)) return
    el.currentTime = Math.min(Math.max(0, seconds), el.duration)
    setTime(el.currentTime)
  }, [])
  const skip = useCallback((by: number) => media.current && seekTo(media.current.currentTime + by), [seekTo])
  const toggleFull = () => {
    const el = box.current
    const v = media.current as (HTMLVideoElement & { webkitEnterFullscreen?: () => void }) | null
    if (document.fullscreenElement) void document.exitFullscreen()
    else if (el?.requestFullscreen) void el.requestFullscreen()
    else v?.webkitEnterFullscreen?.()
  }
  const pictureInPicture = () => {
    const v = media.current
    if (!v || !document.pictureInPictureEnabled) return
    if (document.pictureInPictureElement) void document.exitPictureInPicture()
    else void v.requestPictureInPicture().catch(() => {})
  }

  // Keys that belong to the player; the arrows stay with whoever shows it (a viewer goes to the previous and next file).
  const press = useCallback((e: Press) => {
    if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return
    // Typing in a field is not for the player, and neither is the space that works a button or slider that has the focus.
    const target = e.target as HTMLElement | null
    if (target?.closest('input:not([type="range"]), textarea, select, [contenteditable="true"]')) return
    if (e.key === ' ' && target?.closest('button, [role="slider"], input[type="range"]')) return
    if (e.key === ' ' || e.key === 'k') toggle()
    else if (e.key === 'c') toggleCaptions()
    else if (e.key === 'm') { if (media.current) media.current.muted = !media.current.muted }
    else if (e.key === 'j') skip(-SKIP)
    else if (e.key === 'l') skip(SKIP)
    else return
    e.preventDefault()
    wake()
  }, [toggle, skip, wake, toggleCaptions])
  useEffect(() => {
    if (!globalKeys) return
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (document.querySelector('[data-slot="dialog-content"], [role="menu"]')) return
      press(e)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [globalKeys, press])

  const ratioAt = (e: PointerEvent) => {
    const rect = bar.current?.getBoundingClientRect()
    if (!rect || rect.width === 0) return 0
    return Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
  }
  const onBarKey = (e: KeyboardEvent) => {
    const step = e.key === 'ArrowLeft' ? -5 : e.key === 'ArrowRight' ? 5 : e.key === 'Home' ? -Infinity : e.key === 'End' ? Infinity : 0
    if (!step) return
    e.preventDefault()
    seekTo(step === -Infinity ? 0 : step === Infinity ? duration : time + step)
  }

  const showControls = awake || !playing || menu || scrubbing
  const progress = duration ? (time / duration) * 100 : 0
  const loaded = duration ? (buffered / duration) * 100 : 0
  const VolumeIcon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2
  const captionsOn = subs.selected >= 0
  const shownLines = subs.lines.length > 0 ? subs.lines : previewing ? [t('Sample subtitle')] : []

  return (
    <div
      ref={box}
      tabIndex={-1}
      onKeyDown={(e) => { if (!globalKeys) press(e) }}
      onPointerMove={wake}
      onPointerDown={(e) => { wake(); if (!(e.target as HTMLElement).closest('button, input, [role="slider"]')) box.current?.focus({ preventScroll: true }) }}
      className={cx(
        'group/player relative flex max-h-full max-w-full items-center justify-center overflow-hidden bg-black shadow-2xl ring-1 ring-white/10 outline-none sm:rounded-xl',
        full && 'size-full rounded-none ring-0',
        !showControls && 'cursor-none',
        className,
      )}
    >
      <video
        ref={attach}
        poster={source.poster}
        autoPlay
        playsInline
        aria-label={source.title}
        className={cx('max-h-full max-w-full', full && 'size-full object-contain')}
        onClick={() => { if (menu) setMenu(false); else toggle() }}
        onDoubleClick={toggleFull}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onWaiting={() => setWaiting(true)}
        onPlaying={() => setWaiting(false)}
        onCanPlay={() => setWaiting(false)}
        onLoadStart={() => setWaiting(true)}
        onTimeUpdate={(e) => !scrubbing && setTime(e.currentTarget.currentTime)}
        onDurationChange={(e) => setDuration(Number.isFinite(e.currentTarget.duration) ? e.currentTarget.duration : 0)}
        onProgress={(e) => {
          const ranges = e.currentTarget.buffered
          setBuffered(ranges.length ? ranges.end(ranges.length - 1) : 0)
        }}
        onVolumeChange={(e) => { setVolume(e.currentTarget.volume); setMuted(e.currentTarget.muted) }}
        onRateChange={(e) => setRate(e.currentTarget.playbackRate)}
        onError={() => { if (stream.mode === 'original') onFail() }}
      >
        {subs.tracks.map((s) => (
          <track key={s.index} kind="subtitles" src={s.url} srcLang={s.language || undefined} label={s.name} />
        ))}
      </video>

      <SubtitleOverlay lines={shownLines} style={subs.style} height={height} lift={showControls ? covered : 0} />

      {waiting && (
        <div className="pointer-events-none absolute inset-0 grid place-items-center">
          <Loader2 className="size-10 animate-spin text-white/80 drop-shadow" />
        </div>
      )}
      {!playing && !waiting && (
        <button
          type="button"
          aria-label={t('Play')}
          onClick={toggle}
          className="absolute top-1/2 left-1/2 grid size-16 -translate-x-1/2 -translate-y-1/2 place-items-center rounded-full bg-black/55 text-white backdrop-blur-md ring-1 ring-white/20 transition-transform hover:scale-105 hover:bg-black/70"
        >
          <Play className="size-7 translate-x-0.5 fill-current" />
        </button>
      )}

      <div
        ref={controls}
        className={cx(
          'absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/85 via-black/45 to-transparent px-3 pt-12 pb-2 text-white transition-opacity duration-200',
          showControls ? 'opacity-100' : 'pointer-events-none opacity-0',
        )}
      >
        <div
          ref={bar}
          role="slider"
          tabIndex={0}
          aria-label={t('Seek')}
          aria-valuemin={0}
          aria-valuemax={Math.round(duration)}
          aria-valuenow={Math.round(time)}
          aria-valuetext={`${clock(time)} / ${clock(duration)}`}
          onKeyDown={onBarKey}
          onPointerDown={(e) => {
            e.currentTarget.setPointerCapture(e.pointerId)
            setScrubbing(true)
            seekTo(ratioAt(e) * duration)
          }}
          onPointerMove={(e) => {
            const ratio = ratioAt(e)
            setHover({ x: ratio * 100, at: ratio * duration })
            if (scrubbing) seekTo(ratio * duration)
          }}
          onPointerUp={() => setScrubbing(false)}
          onPointerCancel={() => setScrubbing(false)}
          onPointerLeave={() => setHover(null)}
          className="group/bar relative flex h-5 cursor-pointer touch-none items-center focus-visible:outline-none"
        >
          <div className="relative h-1 w-full rounded-full bg-white/25 transition-[height] group-hover/bar:h-1.5 group-focus-visible/bar:h-1.5">
            <div className="absolute inset-y-0 start-0 rounded-full bg-white/35" style={{ width: `${loaded}%` }} />
            <div className="absolute inset-y-0 start-0 rounded-full bg-primary" style={{ width: `${progress}%` }} />
            <div
              className={cx('absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-primary shadow ring-2 ring-white transition-transform', scrubbing ? 'scale-110' : 'scale-0 group-hover/bar:scale-100 group-focus-visible/bar:scale-100')}
              style={{ left: `${progress}%` }}
            />
          </div>
          {hover && duration > 0 && (
            <div className="pointer-events-none absolute bottom-6 -translate-x-1/2 rounded-md bg-black/80 px-1.5 py-0.5 text-xs tabular-nums" style={{ left: `${hover.x}%` }}>
              {clock(hover.at)}
            </div>
          )}
        </div>

        <div className="flex items-center gap-0.5">
          <button type="button" className={iconButton} aria-label={playing ? t('Pause') : t('Play')} onClick={toggle}>
            {playing ? <Pause className="size-5 fill-current" /> : <Play className="size-5 fill-current" />}
          </button>
          <div className="group/volume flex items-center">
            <button type="button" className={iconButton} aria-label={muted ? t('Unmute') : t('Mute')} onClick={() => { if (media.current) media.current.muted = !media.current.muted }}>
              <VolumeIcon className="size-5" />
            </button>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={muted ? 0 : volume}
              aria-label={t('Volume')}
              onChange={(e) => {
                const v = media.current
                if (!v) return
                v.volume = Number(e.target.value)
                v.muted = Number(e.target.value) === 0
              }}
              className="h-1 w-0 cursor-pointer accent-white opacity-0 transition-all group-focus-within/volume:mx-1 group-focus-within/volume:w-20 group-focus-within/volume:opacity-100 group-hover/volume:mx-1 group-hover/volume:w-20 group-hover/volume:opacity-100"
            />
          </div>
          <span className="ms-1 text-xs tabular-nums text-white/90">{clock(time)} / {clock(duration)}</span>
          <span className="flex-1" />
          {stream.mode === 'stream' && stream.level === AUTO && stream.shown !== null && stream.heights[stream.shown] && (
            <span className="me-1 hidden rounded bg-white/15 px-1.5 py-0.5 text-[11px] font-medium tabular-nums sm:inline">{stream.heights[stream.shown]}p</span>
          )}
          {stream.mode === 'original' && stream.canStream && <span className="me-1 hidden rounded bg-white/15 px-1.5 py-0.5 text-[11px] font-medium sm:inline">{t('Original')}</span>}
          {subs.tracks.length > 0 && (
            <button type="button" className={cx(iconButton, captionsOn && 'bg-white/20')} aria-label={t('Subtitles')} aria-pressed={captionsOn} onClick={toggleCaptions}>
              {captionsOn ? <Captions className="size-5" /> : <CaptionsOff className="size-5" />}
            </button>
          )}
          <div className="relative">
            <button type="button" className={cx(iconButton, menu && 'bg-white/15')} aria-label={t('Settings')} aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu((v) => !v)}>
              <Settings className={cx('size-5 transition-transform', menu && 'rotate-45')} />
            </button>
            {menu && (
              <div
                role="menu"
                // As tall as fits above the controls in this player (a small player scrolls its menu instead of cutting it off).
                style={{ maxHeight: Math.min(384, Math.max(96, height - covered - 8)) }}
                className="absolute end-0 bottom-11 w-60 overflow-y-auto rounded-xl bg-black/85 p-1.5 text-white shadow-2xl ring-1 ring-white/15 backdrop-blur-md"
              >
                <SettingsMenu
                  t={t}
                  source={stream}
                  subs={subs}
                  rate={rate}
                  onRate={(next) => { if (media.current) media.current.playbackRate = next }}
                  close={() => setMenu(false)}
                  onSubtitlePage={setPreviewing}
                />
              </div>
            )}
          </div>
          {typeof document !== 'undefined' && document.pictureInPictureEnabled && (
            <button type="button" className={cx(iconButton, 'max-sm:hidden')} aria-label={t('Picture in picture')} onClick={pictureInPicture}>
              <PictureInPicture2 className="size-5" />
            </button>
          )}
          <button type="button" className={iconButton} aria-label={full ? t('Exit full screen') : t('Full screen')} onClick={toggleFull}>
            {full ? <Minimize className="size-5" /> : <Maximize className="size-5" />}
          </button>
        </div>
      </div>
    </div>
  )
}
