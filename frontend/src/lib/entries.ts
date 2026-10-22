import type { Entry } from './types'

/** Identifies an entry of a listing, for selection. */
export const entryKey = (e: Entry) => `${e.kind}|${e.device}|${e.path}`
