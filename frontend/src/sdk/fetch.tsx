/**
 * What every module that reads sources shows the same way: the card on the Processing page and the fields of the form that every kind of
 * source has. Their checks are in `fetch-limits.ts`. A module's form adds what is its own.
 */
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { useNow } from '@/hooks/use-now'
import { ago } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import type { FileLimits } from './fetch-limits'

/** What a module says of its work (its `GET /status`). */
interface Report {
  slots: number
  made: number
  active: { kind: string; path: string; running: boolean; since: number }[]
  failed: { kind: string; path: string; error: string; at: number }[]
}

/**
 * A module's section of the Processing page: a sentence about the work, then what is being made and what failed. `kinds` names the
 * kinds of task the module reports (a kind it does not name is shown as it is); `children` add lines under the sentence.
 */
export function TaskCard({
  title,
  summary,
  nothing,
  kinds,
  report,
  children,
}: {
  title: string
  summary: (slots: number, made: number) => string
  nothing: string
  kinds: Record<string, string>
  report: unknown
  children?: ReactNode
}) {
  return (
    <Card>
      <CardHeader><CardTitle>{title}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <TaskBody summary={summary} nothing={nothing} kinds={kinds} report={report}>{children}</TaskBody>
      </CardContent>
    </Card>
  )
}

/** What a card says of a module's work: the sentence, the lines the module adds, then what is being made and what failed. */
function TaskBody({ lead, summary, nothing, kinds, report, children }: { lead?: ReactNode; summary: (slots: number, made: number) => string; nothing: string; kinds: Record<string, string>; report: unknown; children?: ReactNode }) {
  const t = useT()
  const now = useNow()
  const { slots, made, active, failed } = report as Report
  return (
    <>
      {lead}
      <p className="text-sm text-muted-foreground">{summary(slots, made)}</p>
      {children}
      {active.length === 0 && failed.length === 0 && <p className="text-sm text-muted-foreground">{nothing}</p>}
      <ul className="divide-y text-sm">
        {active.map((a, i) => (
          <li key={i} className="flex items-center justify-between gap-3 py-1.5">
            <span className="min-w-0 truncate"><Badge variant="secondary" className="mr-2">{t(kinds[a.kind] ?? a.kind)}</Badge>{a.path}</span>
            <span className="shrink-0 text-muted-foreground">{a.running ? t('Running') : t('Waiting')} · {ago(a.since, now)}</span>
          </li>
        ))}
        {failed.map((f, i) => (
          <li key={`f${i}`} className="py-1.5 text-destructive">
            <span className="mr-2 font-medium">{t(kinds[f.kind] ?? f.kind)}</span>{f.path}
            <span className="block truncate text-xs" title={f.error}>{f.error} · {ago(f.at, now)}</span>
          </li>
        ))}
      </ul>
    </>
  )
}

/** The one Downloads card of the Processing page: every module that reads sources adds its part to it (`FetchQueueCard`). */
const Downloads = createContext<{ slot: HTMLElement | null; claim: (by: number) => void } | null>(null)

/** Wraps the module sections of the Processing page; shows the Downloads card while a module has a part in it. */
export function DownloadsProvider({ children }: { children: ReactNode }) {
  const t = useT()
  const [slot, setSlot] = useState<HTMLElement | null>(null)
  const [parts, setParts] = useState(0)
  const claim = useCallback((by: number) => setParts((n) => n + by), [])
  return (
    <Downloads.Provider value={{ slot, claim }}>
      <Card className={cn(parts === 0 && 'hidden')}>
        <CardHeader><CardTitle>{t('Downloads')}</CardTitle></CardHeader>
        <CardContent><div ref={setSlot} className="divide-y" /></CardContent>
      </Card>
      {children}
    </Downloads.Provider>
  )
}

const KIND: Record<string, string> = {
  list: msg('Listing a source'),
  fetch: msg('Fetching a file'),
}

/**
 * The part of a module that reads sources in the Downloads card of the Processing page: sources being listed and files being fetched,
 * under a badge with the module's name (the page passes `module`).
 */
export function FetchQueueCard({ report, module, children }: { report: unknown; module?: string; children?: ReactNode }) {
  const t = useT()
  const downloads = useContext(Downloads)
  const claim = downloads?.claim
  useEffect(() => {
    claim?.(1)
    return () => claim?.(-1)
  }, [claim])
  if (!downloads?.slot) return null
  return createPortal(
    <div className="space-y-3 py-3 first:pt-0 last:pb-0">
      <TaskBody
        lead={module ? <Badge variant="outline" className="w-fit">{module}</Badge> : undefined}
        summary={(slots, made) => t('Fetched as sources run, {slots} at a time. {made} finished since it started.', { slots, made })}
        nothing={t('Nothing is being fetched now.')}
        kinds={KIND}
        report={report}
      >
        {children}
      </TaskBody>
    </div>,
    downloads.slot,
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
