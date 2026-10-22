import { useEffect, useRef, useState } from 'react'
import { monthTitle } from '@/lib/format'
import { cn } from '@/lib/utils'
import { useT } from '@/lib/i18n'

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n))

/**
 * A slim track at the right edge of the timeline: it shows where you are, names the years, and jumps to any
 * month when you drag it or touch it. Positions follow the real heights of the months, so a busy month is
 * a longer stretch of the track.
 */
export function Scrubber({
  months,
  starts,
  total,
  scrollMargin,
}: {
  months: { month: string; count: number }[]
  /** Top of each month inside the list, in pixels. */
  starts: number[]
  /** Height of the whole list. */
  total: number
  /** Where the list begins on the page. */
  scrollMargin: number
}) {
  const t = useT()
  const track = useRef<HTMLDivElement>(null)
  const [height, setHeight] = useState(0)
  const [scrollY, setScrollY] = useState(0)
  const [hover, setHover] = useState<number | null>(null)
  const [dragging, setDragging] = useState(false)
  // Only worth having when there is more than a screenful to scroll through.
  const ready = months.length > 1 && total > 0

  // The track is always in the page (hidden until it is useful), so that it is measured from the start.
  useEffect(() => {
    const el = track.current
    if (!el) return
    setHeight(el.clientHeight)
    const observer = new ResizeObserver(() => setHeight(el.clientHeight))
    observer.observe(el)
    let frame = 0
    const onScroll = () => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => setScrollY(window.scrollY))
    }
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      observer.disconnect()
      cancelAnimationFrame(frame)
      window.removeEventListener('scroll', onScroll)
    }
  }, [])

  const fractionOf = (px: number) => clamp(px / (total || 1), 0, 1)
  const here = fractionOf(scrollY - scrollMargin)
  const monthAt = (fraction: number) => {
    let found = 0
    for (let i = 0; i < starts.length; i++) if (starts[i] <= fraction * total) found = i
    return found
  }
  const fractionAt = (e: React.PointerEvent) => {
    const r = track.current?.getBoundingClientRect()
    return r && r.height > 0 ? clamp((e.clientY - r.top) / r.height, 0, 1) : 0
  }
  const jump = (e: React.PointerEvent) => {
    const f = fractionAt(e)
    setHover(f)
    window.scrollTo({ top: scrollMargin + f * total })
  }

  // A year gets a label where it begins, if there is room for the text.
  const labels: { top: number; text: string }[] = []
  if (ready) {
    let lastTop = -100
    months.forEach((m, i) => {
      const year = m.month.slice(0, 4)
      if (i > 0 && months[i - 1].month.slice(0, 4) === year) return
      const top = ((starts[i] ?? 0) / total) * height
      if (top - lastTop >= 22) {
        labels.push({ top, text: year })
        lastTop = top
      }
    })
  }

  const active = dragging || hover !== null
  const current = ready ? monthAt(here) : 0
  const shown = ready ? monthAt(active ? (hover ?? here) : here) : 0
  return (
    <div
      ref={track}
      role="slider"
      tabIndex={ready ? 0 : -1}
      aria-label={t('Scroll through time')}
      aria-orientation="vertical"
      aria-valuemin={0}
      aria-valuemax={Math.max(0, months.length - 1)}
      aria-valuenow={current}
      aria-valuetext={ready ? monthTitle(months[current].month) : undefined}
      onKeyDown={(e) => {
        const step = e.key === 'ArrowDown' ? 1 : e.key === 'ArrowUp' ? -1 : 0
        if (!step || !ready) return
        e.preventDefault()
        const i = clamp(current + step, 0, months.length - 1)
        window.scrollTo({ top: scrollMargin + (starts[i] ?? 0) })
      }}
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId)
        setDragging(true)
        jump(e)
      }}
      onPointerMove={(e) => {
        if (dragging) jump(e)
        else if (e.pointerType === 'mouse') setHover(fractionAt(e))
      }}
      onPointerUp={() => {
        setDragging(false)
        setHover(null)
      }}
      onPointerCancel={() => {
        setDragging(false)
        setHover(null)
      }}
      onPointerLeave={() => !dragging && setHover(null)}
      className={cn('fixed top-16 right-0 bottom-3 z-30 w-9 touch-none select-none outline-offset-[-2px] focus-visible:outline-2 focus-visible:outline-ring', !ready && 'hidden')}
    >
      {labels.map((l) => (
        <span key={l.text} className="pointer-events-none absolute right-2 -translate-y-px text-[10px] font-medium text-muted-foreground tabular-nums" style={{ top: l.top }}>
          {l.text}
        </span>
      ))}
      {ready && (
        <>
          <span
            className={cn('pointer-events-none absolute right-0 h-1 w-7 rounded-l bg-primary shadow transition-[width]', active && 'w-9')}
            style={{ top: clamp(here * height - 2, 0, Math.max(0, height - 4)) }}
          />
          {active && (
            <span
              className="pointer-events-none absolute right-10 -translate-y-1/2 rounded-md bg-popover px-2.5 py-1 text-xs font-medium whitespace-nowrap text-popover-foreground shadow-md ring-1 ring-border"
              style={{ top: clamp((hover ?? here) * height, 14, Math.max(14, height - 14)) }}
            >
              {monthTitle(months[shown].month)}
            </span>
          )}
        </>
      )}
    </div>
  )
}
