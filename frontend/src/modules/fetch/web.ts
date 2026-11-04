import { acknowledgedProblem, isWebAddress, limitsBody, limitsFrom, limitsProblem, NO_LIMITS, splitLines, type FileLimits, type T } from '@/sdk'

/** The settings of a web source, as the form holds them. */
export interface Web extends FileLimits {
  /** One address of a file per line. */
  urls: string
  feed: string
  acknowledged: boolean
}

export const NO_WEB: Web = { urls: '', feed: '', ...NO_LIMITS, acknowledged: false }

/** The part of the API body that holds the settings. */
export function webBody(s: Web) {
  return { urls: splitLines(s.urls), feed: s.feed.trim(), ...limitsBody(s), acknowledged: s.acknowledged }
}

/** The form's settings from what the master gave back. */
export function webLoad(c: Record<string, unknown>): Web {
  return { urls: Array.isArray(c.urls) ? c.urls.join('\n') : '', feed: typeof c.feed === 'string' ? c.feed : '', ...limitsFrom(c), acknowledged: true }
}

/** Why the settings cannot be used yet, or null. */
export function webProblem(s: Web, t: T): string | null {
  const list = splitLines(s.urls)
  if (list.length === 0 && !s.feed.trim()) return t('Give at least one web address of a file, or a feed.')
  if (!list.every(isWebAddress)) return t('Every line must be a web address starting with http:// or https://.')
  if (s.feed.trim() && !isWebAddress(s.feed.trim())) return t('Paste a web address starting with http:// or https://.')
  return limitsProblem(s, t) ?? acknowledgedProblem(s.acknowledged, t)
}
