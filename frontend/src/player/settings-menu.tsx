import { FolderOpen, RotateCcw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Back, Choice, Door, Heading, Segments, Slider } from './menu'
import { COLORS, EDGES, FONTS, LIMITS } from './subtitle-style'
import type { Translate } from './types'
import type { SourceState } from './use-source'
import { AUTO } from './use-source'
import type { Subtitles } from './use-subtitles'
import { cx, signedSeconds } from './util'

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2]

interface Props {
  t: Translate
  source: SourceState
  subs: Subtitles
  rate: number
  onRate: (rate: number) => void
  close: () => void
  /** Tells the player when the subtitle page is open, so that it can show a sample line to judge the look by. */
  onSubtitlePage: (open: boolean) => void
}

/** The panel of the gear button: quality, subtitles (which one, how they look, how far they are moved) and speed. */
export function SettingsMenu({ t, source, subs, rate, onRate, close, onSubtitlePage }: Props) {
  const [page, setPage] = useState<'main' | 'subtitles'>('main')
  const picker = useRef<HTMLInputElement>(null)
  useEffect(() => {
    onSubtitlePage(page === 'subtitles')
    return () => onSubtitlePage(false)
  }, [page, onSubtitlePage])

  const { mode, level, shown, heights } = source
  const qualities = heights.map((h, i) => ({ h, i })).sort((a, b) => b.h - a.h)
  const chosen = subs.tracks.find((s) => s.index === subs.selected)?.name ?? t('Off')

  if (page === 'subtitles') {
    const { style } = subs
    const edgeNames = { none: t('None'), outline: t('Outline'), shadow: t('Shadow') }
    const fontNames = { sans: t('Sans'), serif: t('Serif'), mono: t('Mono') }
    return (
      <>
        <Back label={t('Subtitles')} onBack={() => setPage('main')} />
        <Choice on={subs.selected < 0} onPick={() => subs.select(-1)}>{t('Off')}</Choice>
        {subs.tracks.map((s) => (
          <Choice key={s.index} on={subs.selected === s.index} onPick={() => subs.select(s.index)}>{s.name}</Choice>
        ))}
        <input
          ref={picker}
          type="file"
          accept=".srt,.vtt,text/vtt,application/x-subrip"
          hidden
          onChange={(e) => {
            const file = e.target.files?.[0]
            e.target.value = ''
            if (file) void subs.addFile(file)
          }}
        />
        <button
          type="button"
          role="menuitem"
          onClick={() => picker.current?.click()}
          className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm hover:bg-white/15 focus-visible:bg-white/15 focus-visible:outline-none"
        >
          <FolderOpen className="size-4 shrink-0" />
          <span className="truncate">{t('Load from a file…')}</span>
        </button>

        <Heading>{t('Appearance')}</Heading>
        <Slider label={t('Size')} value={style.size} shown={`${style.size}%`} {...{ min: LIMITS.size[0], max: LIMITS.size[1], step: 10 }} onChange={(size) => subs.changeStyle({ size })} />
        <div className="px-2 py-1" role="radiogroup" aria-label={t('Text color')}>
          <span className="text-xs text-white/80">{t('Text color')}</span>
          <div className="mt-1 flex gap-2">
            {COLORS.map((color) => (
              <button
                key={color}
                type="button"
                role="radio"
                aria-checked={style.color === color}
                aria-label={color}
                onClick={() => subs.changeStyle({ color })}
                style={{ backgroundColor: color }}
                className={cx('size-6 rounded-full ring-2 ring-offset-1 ring-offset-black/80 focus-visible:outline-none', style.color === color ? 'ring-white' : 'ring-transparent hover:ring-white/50')}
              />
            ))}
          </div>
        </div>
        <Slider label={t('Background')} value={style.opacity} shown={`${Math.round(style.opacity * 100)}%`} {...{ min: LIMITS.opacity[0], max: LIMITS.opacity[1], step: 0.05 }} onChange={(opacity) => subs.changeStyle({ opacity })} />
        <Segments label={t('Edge')} value={style.edge} options={EDGES.map((value) => ({ value, label: edgeNames[value] }))} onChange={(edge) => subs.changeStyle({ edge })} />
        <Segments label={t('Font')} value={style.font} options={FONTS.map((value) => ({ value, label: fontNames[value] }))} onChange={(font) => subs.changeStyle({ font })} />
        <Slider label={t('Height')} value={style.position} shown={`${style.position}%`} {...{ min: LIMITS.position[0], max: LIMITS.position[1], step: 1 }} onChange={(position) => subs.changeStyle({ position })} />

        <Heading>{t('Timing')}</Heading>
        <div className="flex items-center gap-1 px-2 py-1">
          <span className="flex-1 text-xs text-white/80">{t('Delay')}</span>
          <button type="button" aria-label={t('Earlier')} onClick={() => subs.changeDelay(subs.delay - 0.1)} className="grid size-7 place-items-center rounded-md bg-white/10 hover:bg-white/20 focus-visible:bg-white/20 focus-visible:outline-none">−</button>
          <span className="w-14 text-center text-xs tabular-nums">{signedSeconds(subs.delay)}</span>
          <button type="button" aria-label={t('Later')} onClick={() => subs.changeDelay(subs.delay + 0.1)} className="grid size-7 place-items-center rounded-md bg-white/10 hover:bg-white/20 focus-visible:bg-white/20 focus-visible:outline-none">+</button>
        </div>
        <button
          type="button"
          role="menuitem"
          onClick={subs.resetStyle}
          className="mt-1 flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm hover:bg-white/15 focus-visible:bg-white/15 focus-visible:outline-none"
        >
          <RotateCcw className="size-4 shrink-0" />
          <span className="truncate">{t('Reset the look and the delay')}</span>
        </button>
      </>
    )
  }

  return (
    <>
      {source.canStream && (
        <>
          <Heading>{t('Quality')}</Heading>
          <Choice on={mode === 'stream' && level === AUTO} onPick={() => { source.chooseStream(); close() }}>
            {t('Auto')}{mode === 'stream' && level === AUTO && shown !== null && heights[shown] ? ` · ${heights[shown]}p` : ''}
          </Choice>
          {qualities.map(({ h, i }) => (
            <Choice key={i} on={mode === 'stream' && level === i} onPick={() => { source.pickLevel(i); close() }}>{h}p</Choice>
          ))}
          <Choice on={mode === 'original'} onPick={() => { source.chooseOriginal(); close() }}>{t('Original')}</Choice>
        </>
      )}
      <Heading>{t('Subtitles')}</Heading>
      <Door label={t('Subtitles')} value={chosen} onOpen={() => setPage('subtitles')} />
      <Heading>{t('Speed')}</Heading>
      {SPEEDS.map((s) => (
        <Choice key={s} on={rate === s} onPick={() => { onRate(s); close() }}>
          {s === 1 ? t('Normal') : `${s}×`}
        </Choice>
      ))}
    </>
  )
}
