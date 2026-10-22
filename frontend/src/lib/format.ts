const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']

export function bytes(n: number | null | undefined): string {
  if (n === null || n === undefined) return '—'
  let value = n
  let unit = 0
  while (value >= 1000 && unit < UNITS.length - 1) {
    value /= 1000
    unit++
  }
  return `${unit === 0 || value >= 100 ? Math.round(value) : value.toFixed(1)} ${UNITS[unit]}`
}

export function duration(seconds: number): string {
  if (seconds < 90) return `${Math.round(seconds)} s`
  if (seconds < 5400) return `${Math.round(seconds / 60)} min`
  if (seconds < 172800) return `${Math.round(seconds / 3600)} h`
  return `${Math.round(seconds / 86400)} days`
}

/** "3 min ago", "just now"; `now` is passed in so that a clock tick can refresh it. */
export function ago(ts: number | null | undefined, now = Date.now() / 1000): string {
  if (!ts) return 'never'
  const d = now - ts
  if (d < 10) return 'just now'
  return `${duration(d)} ago`
}

export function dateTime(ts: number | null | undefined): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
}

export function date(ts: number | null | undefined): string {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleDateString(undefined, { dateStyle: 'medium' })
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December']
const WEEKDAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']

// Photos carry no time zone, so the library reads and shows every date as UTC: the same on every screen.

/** "2024-05" from unix seconds. */
export function monthKey(ts: number): string {
  const d = new Date(ts * 1000)
  return `${String(d.getUTCFullYear()).padStart(4, '0')}-${String(d.getUTCMonth() + 1).padStart(2, '0')}`
}

/** "2024-05-17" from unix seconds. */
export function dayKey(ts: number): string {
  return `${monthKey(ts)}-${String(new Date(ts * 1000).getUTCDate()).padStart(2, '0')}`
}

/** "May 2024" from "2024-05". */
export function monthTitle(month: string): string {
  const [y, m] = month.split('-')
  return `${MONTHS[Number(m) - 1] ?? m} ${Number(y)}`
}

/** "Friday, 17 May 2024" from a day key. */
export function dayTitle(day: string): string {
  const [y, m, d] = day.split('-').map(Number)
  const weekday = WEEKDAYS[new Date(Date.UTC(y, m - 1, d)).getUTCDay()]
  return `${weekday}, ${d} ${MONTHS[m - 1]} ${y}`
}

/** "17 May 2024, 14:03" as recorded. */
export function whenText(ts: number): string {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}, ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`
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
export function clock(ms: number): string {
  const total = Math.round(ms / 1000)
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}` : `${m}:${String(s).padStart(2, '0')}`
}

export function count(n: number): string {
  return n.toLocaleString()
}

/** "1 file", "2 files". */
export function plural(n: number, word: string): string {
  return `${count(n)} ${word}${n === 1 ? '' : 's'}`
}

export function percent(done: number, total: number): number | null {
  return total > 0 ? Math.min(100, Math.floor((100 * done) / total)) : null
}

/** "docs/a/b.txt" -> "b.txt" */
export function baseName(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1)
}

/** "docs/a/b.txt" -> "docs/a"; "" for a top-level path */
export function parentPath(path: string): string {
  const i = path.lastIndexOf('/')
  return i < 0 ? '' : path.slice(0, i)
}

export function joinPath(dir: string, name: string): string {
  return dir ? `${dir}/${name}` : name
}
