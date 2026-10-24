import { msg } from './i18n'

/** The words for the states things are in (jobs, devices, sources, mirrors); show one with `t(STATUS[state])`. */
export const STATUS: Record<string, string> = {
  ok: msg('OK'),
  online: msg('Online'),
  partial: msg('Partial'),
  paused: msg('Paused'),
  running: msg('Running'),
  failed: msg('Failed'),
  offline: msg('Offline'),
  revoked: msg('Revoked'),
  'never run': msg('Never run'),
}
