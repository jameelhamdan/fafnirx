import { clock as clockOfSeconds } from '@/player/util'
import { locale, msg, tr } from './i18n'

const UNITS = [msg('B'), msg('KB'), msg('MB'), msg('GB'), msg('TB'), msg('PB')]

export function bytes(n: number | null | undefined): string {
  if (n === null || n === undefined) return '—'
  let value = n
  let unit = 0
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit++
  }
  return `${unit === 0 || value >= 100 ? Math.round(value) : value.toFixed(1)} ${tr(UNITS[unit])}`
}

export function duration(seconds: number): string {
  if (seconds < 90) return tr('{n} s', { n: Math.round(seconds) })
  if (seconds < 5400) return tr('{n} min', { n: Math.round(seconds / 60) })
  if (seconds < 172800) return tr('{n} h', { n: Math.round(seconds / 3600) })
  return tr('{n} days', { n: Math.round(seconds / 86400) })
}

/** "3 min ago", "just now"; `now` is passed in so that a clock tick can refresh it. */
export function ago(ts: number | null | undefined, now = Date.now() / 1000): string {
  if (!ts) return tr('never')
  const d = now - ts
  if (d < 10) return tr('just now')
  return tr('{time} ago', { time: duration(d) })
}

export function dateTime(ts: number | null | undefined): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleString(locale(), { dateStyle: 'medium', timeStyle: 'short' })
}

export function date(ts: number | null | undefined): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleDateString(locale(), { dateStyle: 'medium' })
}

// Photos carry no time zone, so the library reads and shows every date as UTC: the same on every screen.

/** "2024-05" from unix seconds. */
function monthKey(ts: number): string {
  const d = new Date(ts * 1000)
  return `${String(d.getUTCFullYear()).padStart(4, '0')}-${String(d.getUTCMonth() + 1).padStart(2, '0')}`
}

/** "2024-05-17" from unix seconds. */
export function dayKey(ts: number): string {
  return `${monthKey(ts)}-${String(new Date(ts * 1000).getUTCDate()).padStart(2, '0')}`
}

/** "May 2024" from "2024-05". */
export function monthTitle(month: string): string {
  const [y, m] = month.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(locale(), { month: 'long', year: 'numeric', timeZone: 'UTC' })
}

/** "Friday, 17 May 2024" from a day key. */
export function dayTitle(day: string): string {
  const [y, m, d] = day.split('-').map(Number)
  return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString(locale(), { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' })
}

/** "17 May 2024, 14:03" as recorded. */
export function whenText(ts: number): string {
  return new Date(ts * 1000).toLocaleString(locale(), { day: 'numeric', month: 'long', year: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false, timeZone: 'UTC' })
}

/** The value of a `datetime-local` input for unix seconds (and back). */
export function toInputValue(ts: number): string {
  return new Date(ts * 1000).toISOString().slice(0, 19)
}

export function fromInputValue(value: string): number | null {
  const ms = Date.parse(`${value}Z`)
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000)
}

/** "1:23" or "1:02:03" from milliseconds. */
/** A length of time given in milliseconds, as `1:05` or `1:02:03`. */
export const clock = (ms: number): string => clockOfSeconds(Math.round(ms / 1000))

export function count(n: number): string {
  return n.toLocaleString(locale())
}

const PLURALS: Record<string, [string, string]> = {
  file: [msg('{count} file'), msg('{count} files')],
  item: [msg('{count} item'), msg('{count} items')],
  'photo or video': [msg('{count} photo or video'), msg('{count} photos or videos')],
}

/** "1 file", "2 files". */
export function plural(n: number, word: keyof typeof PLURALS): string {
  return tr(PLURALS[word][n === 1 ? 0 : 1], { count: count(n) })
}

export function percent(done: number, total: number): number | null {
  return total > 0 ? Math.min(100, Math.floor((100 * done) / total)) : null
}

export function joinPath(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name
}
