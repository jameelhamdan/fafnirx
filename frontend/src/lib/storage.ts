import type { useT } from '@/lib/i18n'
import { PRESETS } from '@/lib/mirror-presets'

/** The storage a mirror writes to or a source reads from, as the form holds it. */
export interface Storage {
  preset: string
  values: Record<string, string>
  path: string
  advanced: string
}

export const NO_STORAGE: Storage = { preset: 'b2', values: {}, path: '', advanced: '' }

export const presetOf = (s: Storage) => PRESETS.find((p) => p.id === s.preset) ?? PRESETS[0]

/** The part of the API body that names the storage. */
export function storageBody(s: Storage) {
  const preset = presetOf(s)
  const plain = preset.fields.filter((f) => !f.obscure).map((f) => [f.key, (s.values[f.key] ?? '').trim()] as const)
  const lines = [...plain.filter(([, v]) => v).map(([k, v]) => `${k}=${v}`), s.advanced.trim()].filter(Boolean)
  return {
    remote: preset.id === 'conf' ? s.path.trim() : '',
    remote_type: preset.type,
    remote_path: preset.id === 'conf' ? '' : s.path.trim(),
    options: lines.join('\n'),
    secrets: Object.fromEntries(preset.fields.filter((f) => f.obscure).map((f) => [f.key, s.values[f.key] ?? '']).filter(([, v]) => v)),
  }
}

/** Why the storage cannot be used yet, or null. */
export function storageProblem(s: Storage, t: ReturnType<typeof useT>): string | null {
  const preset = presetOf(s)
  const missing = preset.fields.find((f) => !f.optional && !(s.values[f.key] ?? '').trim())
  if (missing) return t('Fill in: {field}', { field: t(missing.label) })
  if (!s.path.trim() && !preset.pathOptional) return t('Say where to put the files.')
  if (preset.id === 'conf' && !s.path.includes(':')) return t('Write the remote as name:folder.')
  return null
}
