/** Joins the class names that are given. */
export const cx = (...parts: (string | false | null | undefined)[]) => parts.filter(Boolean).join(' ')

/** `1:05` or `1:02:03`. */
export function clock(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) return '0:00'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  const s = String(Math.floor(seconds % 60)).padStart(2, '0')
  return h ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

/** `+0.5 s`, `-1.2 s`, `0 s`. */
export const signedSeconds = (n: number) => `${n > 0 ? '+' : ''}${Number(n.toFixed(1))} s`

/** What the browser remembers for the player; storage can be blocked, and then nothing is remembered. */
export function recall(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

export function remember(key: string, value: string) {
  try {
    localStorage.setItem(key, value)
  } catch { /* storage can be blocked */ }
}
