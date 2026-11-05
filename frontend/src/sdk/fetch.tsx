/**
 * What every module that reads sources shows the same way: the fields of the form that every kind of source has. Their checks are in
 * `fetch-limits.ts`. A module's form adds what is its own.
 */
import type { ReactNode } from 'react'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { msg, useT } from '@/lib/i18n'
import type { FileLimits } from './fetch-limits'

export function TextField({ id, label, value, onChange, placeholder }: { id: string; label: string; value: string; onChange: (value: string) => void; placeholder?: string }) {
  const t = useT()
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{t(label)}</Label>
      <Input id={id} autoComplete="off" placeholder={placeholder} value={value} onChange={(e) => onChange(e.target.value)} />
    </div>
  )
}

export function PerRunField({ id, value, onChange, hint }: { id: string; value: string; onChange: (value: string) => void; hint?: string }) {
  const t = useT()
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{t('Files per run')}</Label>
      <Input id={id} inputMode="numeric" value={value} onChange={(e) => onChange(e.target.value)} />
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
    </div>
  )
}

/** The fields of [`FileLimits`]; `children` are the module's own, in the same grid. `placeholder` is an example of a name pattern. */
export function FileLimitsFields({ idPrefix, value, onChange, placeholder, children }: { idPrefix: string; value: FileLimits; onChange: (patch: Partial<FileLimits>) => void; placeholder: string; children?: ReactNode }) {
  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <TextField id={`${idPrefix}-include`} label={msg('Only files matching (pattern)')} placeholder={placeholder} value={value.include} onChange={(include) => onChange({ include })} />
      <TextField id={`${idPrefix}-exclude`} label={msg('Never files matching (pattern)')} placeholder="(?i)sample|\.nfo$" value={value.exclude} onChange={(exclude) => onChange({ exclude })} />
      <TextField id={`${idPrefix}-min`} label={msg('Smallest file (MB)')} value={value.minMb} onChange={(minMb) => onChange({ minMb })} />
      <TextField id={`${idPrefix}-max`} label={msg('Largest file (GB)')} value={value.maxGb} onChange={(maxGb) => onChange({ maxGb })} />
      <PerRunField id={`${idPrefix}-per`} value={value.perRun} onChange={(perRun) => onChange({ perRun })} />
      {children}
    </div>
  )
}

/** The checkbox with which the user says they may copy what the source downloads; `children` say it in the words of the source. */
export function AcknowledgeField({ checked, onChange, children }: { checked: boolean; onChange: (checked: boolean) => void; children: ReactNode }) {
  return (
    <label className="flex items-start gap-2 text-sm">
      <input type="checkbox" className="mt-0.5 size-4" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>{children}</span>
    </label>
  )
}

/** A template for the name of a file a source makes, with the tokens the module offers (`{title}`). Empty means the module's own default. */
export function NameRuleField({ id, value, onChange, tokens, fallback }: { id: string; value: string; onChange: (value: string) => void; tokens: string[]; fallback: string }) {
  const t = useT()
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{t('File name')}</Label>
      <Input id={id} autoComplete="off" placeholder={fallback} value={value} onChange={(e) => onChange(e.target.value)} />
      <p className="text-xs text-muted-foreground">{t('Written with {tokens}. Leave empty for {fallback}.', { tokens: tokens.map((x) => `{${x}}`).join(' '), fallback })}</p>
    </div>
  )
}
