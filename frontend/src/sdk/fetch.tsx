/**
 * What every module that reads sources shows the same way: the card on the Processing page and the fields of the form that every kind of
 * source has. Their checks are in `fetch-limits.ts`. A module's form adds what is its own.
 */
import type { ReactNode } from 'react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useNow } from '@/hooks/use-now'
import { ago } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import type { FileLimits } from './fetch-limits'

/** What a module that reads sources says of its work (its `GET /status`). */
interface Report {
  slots: number
  made: number
  active: { kind: string; path: string; running: boolean; since: number }[]
  failed: { kind: string; path: string; error: string; at: number }[]
}

const KIND: Record<string, string> = {
  list: msg('Listing a source'),
  fetch: msg('Fetching a file'),
}

/** The module's section of the Processing page: sources being listed and files being fetched. `children` add lines under the summary. */
export function FetchQueueCard({ report, children }: { report: unknown; children?: ReactNode }) {
  const t = useT()
  const now = useNow()
  const { slots, made, active, failed } = report as Report
  return (
    <Card>
      <CardHeader><CardTitle>{t('Downloads')}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          {t('Fetched as sources run, {slots} at a time. {made} finished since it started.', { slots, made })}
        </p>
        {children}
        {active.length === 0 && failed.length === 0 && <p className="text-sm text-muted-foreground">{t('Nothing is being fetched now.')}</p>}
        <ul className="divide-y text-sm">
          {active.map((a, i) => (
            <li key={i} className="flex items-center justify-between gap-3 py-1.5">
              <span className="min-w-0 truncate"><Badge variant="secondary" className="mr-2">{t(KIND[a.kind] ?? a.kind)}</Badge>{a.path}</span>
              <span className="shrink-0 text-muted-foreground">{a.running ? t('Running') : t('Waiting')} · {ago(a.since, now)}</span>
            </li>
          ))}
          {failed.map((f, i) => (
            <li key={`f${i}`} className="py-1.5 text-destructive">
              <span className="mr-2 font-medium">{t(KIND[f.kind] ?? f.kind)}</span>{f.path}
              <span className="block truncate text-xs" title={f.error}>{f.error} · {ago(f.at, now)}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}

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
