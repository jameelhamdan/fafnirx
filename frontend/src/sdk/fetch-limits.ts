/** What every module that reads sources checks the same way: what to keep of the files, how many per run, and the confirmation that the
 * user may copy them. The fields that show them are in `fetch.tsx`. */
import type { T } from '@/lib/i18n'

/** The lines of a text that are not empty, trimmed. */
export const splitLines = (text: string) => text.split('\n').map((l) => l.trim()).filter(Boolean)

/** Whether a text is a web address. */
export const isWebAddress = (text: string) => /^https?:\/\/\S+$/i.test(text)

const optionalNumber = (text: string) => (text.trim() === '' ? undefined : Number(text))

/** What every kind of source lets the user say about the files: which to keep, how big, how many per run. */
export interface FileLimits {
  include: string
  exclude: string
  minMb: string
  maxGb: string
  perRun: string
}

export const NO_LIMITS: FileLimits = { include: '', exclude: '', minMb: '', maxGb: '', perRun: '25' }

/** The part of the API body that holds the limits. */
export function limitsBody(l: FileLimits) {
  return { include: l.include.trim(), exclude: l.exclude.trim(), min_mb: optionalNumber(l.minMb), max_gb: optionalNumber(l.maxGb), max_per_run: Number(l.perRun) }
}

/** Why the files per run cannot be used, or null. */
export function perRunProblem(perRun: string, t: T): string | null {
  const per = Number(perRun)
  return Number.isInteger(per) && per >= 1 && per <= 500 ? null : t('Files per run: a number from 1 to 500.')
}

/** Why the limits cannot be used yet, or null. */
export function limitsProblem(l: FileLimits, t: T): string | null {
  const per = perRunProblem(l.perRun, t)
  if (per) return per
  return [l.minMb, l.maxGb].every((n) => n.trim() === '' || /^\d+$/.test(n.trim())) ? null : t('Sizes are whole numbers.')
}

/** Why the confirmation that the user may copy what the source downloads is missing, or null. */
export const acknowledgedProblem = (acknowledged: boolean, t: T): string | null =>
  acknowledged ? null : t('Confirm that you have the right to copy what this downloads.')
