import { useLayoutEffect, useState, type RefObject } from 'react'
import { dayKey } from './format'

export const GAP = 4

/** How many square cells fit in the element, and how wide each is. Fewer, larger cells on a phone. */
export function useGrid(ref: RefObject<HTMLElement | null>) {
  const [width, setWidth] = useState(0)
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    setWidth(el.clientWidth)
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width))
    observer.observe(el)
    return () => observer.disconnect()
  }, [ref])
  const target = width < 640 ? 104 : 148
  const cols = Math.max(2, Math.floor((width + GAP) / (target + GAP)))
  const cell = width > 0 ? (width - GAP * (cols - 1)) / cols : target
  return { cols, cell, width }
}

// The heights of the parts of a month, as the CSS of `MonthSection` and `DayGrid` makes them.
const MONTH_HEAD = 44
const MONTH_TAIL = 8
const DAY_HEAD = 26
const DAY_TAIL = 16

/** How tall a month is, from how many files each of its days holds (no picture needs to be loaded). */
export function monthHeight(days: number[], cols: number, cell: number): number {
  return (
    MONTH_HEAD +
    MONTH_TAIL +
    days.reduce((height, n) => {
      const rows = Math.ceil(n / cols)
      return height + DAY_HEAD + rows * cell + (rows - 1) * GAP + DAY_TAIL
    }, 0)
  )
}

/** The part of a month's height below its heading and above its tail: where the days are. */
export const monthBodyHeight = (days: number[], cols: number, cell: number) => monthHeight(days, cols, cell) - MONTH_HEAD - MONTH_TAIL

/** Splits assets (already newest first) into days. */
export function groupByDay<T extends { when: number }>(assets: T[]): { day: string; assets: T[] }[] {
  const days: { day: string; assets: T[] }[] = []
  for (const a of assets) {
    const day = dayKey(a.when)
    const last = days[days.length - 1]
    if (last?.day === day) last.assets.push(a)
    else days.push({ day, assets: [a] })
  }
  return days
}
