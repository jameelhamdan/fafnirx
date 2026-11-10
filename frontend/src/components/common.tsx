import { AlertTriangle } from 'lucide-react'
import { useEffect, type ReactNode } from 'react'
import { bytes, duration, percent } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import type { Activity } from '@/lib/types'
import { cn } from '@/lib/utils'
import { Badge } from './ui/badge'
import { Progress } from './ui/progress'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from './ui/select'
import { STATUS } from '@/lib/status'

/** A small label over a value, for a list of facts about one thing; an empty value shows as a dash. */
export function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs font-medium text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 break-words text-sm">{children === null || children === undefined || children === '' ? '—' : children}</dd>
    </div>
  )
}

const PHASE: Record<string, string> = { scanning: msg('Scanning'), hashing: msg('Hashing'), uploading: msg('Uploading') }

const EMPTY = '__empty__'

/**
 * A dropdown over `[value, label]` pairs, for forms and table toolbars. The empty string is a valid value (the "any" or default
 * choice); with none of that value among the options it shows `placeholder` instead.
 */
export function Picker({ value, onChange, options, id, label, placeholder, size, className }: {
  value: string
  onChange: (value: string) => void
  options: [value: string, label: ReactNode][]
  id?: string
  label?: string
  placeholder?: string
  size?: 'sm' | 'default'
  className?: string
}) {
  const has = options.some(([v]) => v === '')
  return (
    <Select value={value === '' && has ? EMPTY : value} onValueChange={(v) => onChange(v === EMPTY ? '' : v)}>
      <SelectTrigger id={id} aria-label={label} size={size} className={className}>
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map(([v, text]) => <SelectItem key={v || EMPTY} value={v === '' ? EMPTY : v}>{text}</SelectItem>)}
      </SelectContent>
    </Select>
  )
}

export function PageHeader({ title, description, actions }: { title: string; description?: ReactNode; actions?: ReactNode }) {
  useEffect(() => {
    document.title = `${title} · fafnirx`
  }, [title])
  return (
    <div className="mb-4 flex flex-wrap items-end justify-between gap-3 border-b pb-3">
      <div>
        <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
        {description && <p className="mt-0.5 max-w-3xl text-[13px] text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  )
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-1 rounded-md border border-dashed py-10 text-center">
      <p className="font-medium">{title}</p>
      {children && <p className="max-w-md text-sm text-muted-foreground">{children}</p>}
    </div>
  )
}

export function ErrorNote({ error }: { error: unknown }) {
  const t = useT()
  return (
    <div className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-3 text-sm text-destructive">
      <AlertTriangle className="size-4 shrink-0" />
      {error instanceof Error ? error.message : t('Something went wrong')}
    </div>
  )
}

const DOT: Record<string, string> = {
  ok: 'bg-success',
  online: 'bg-success',
  partial: 'bg-warning',
  paused: 'bg-warning',
  running: 'bg-primary animate-pulse',
  failed: 'bg-destructive',
  offline: 'bg-muted-foreground/50',
  revoked: 'bg-destructive',
  'never run': 'bg-muted-foreground/50',
}

export function StatusDot({ status, label }: { status: string; label?: string }) {
  const t = useT()
  return (
    <span className="inline-flex items-center gap-1.5 text-[13px]">
      <span className={cn('size-1.5 rounded-full', DOT[status] ?? 'bg-muted-foreground/50')} />
      {label ?? t(STATUS[status] ?? status)}
    </span>
  )
}

const KIND: Record<string, string> = { device: msg('Device'), local: msg('Local folder'), master: msg('Master folder') }

export function KindBadge({ kind }: { kind: string }) {
  const t = useT()
  return <Badge variant="secondary">{t(KIND[kind] ?? kind)}</Badge>
}

/** What a device is doing right now: phase, file, a progress bar, speed and time left. */
export function ActivityView({ activity, compact = false }: { activity: Activity; compact?: boolean }) {
  const t = useT()
  const pct = percent(activity.bytes_done, activity.bytes_total)
  const left = activity.speed_bps > 0 ? (activity.bytes_total - activity.bytes_done) / activity.speed_bps : null
  const phase = t(PHASE[activity.phase] ?? activity.phase)
  return (
    <div className="min-w-0 space-y-1.5">
      <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
        <span className="truncate">
          {phase}
          {activity.files_total > 0 && ` ${Math.min(activity.files_done + 1, activity.files_total)}/${activity.files_total}`}
          {!compact && activity.file && `: ${activity.file}`}
        </span>
        <span className="num shrink-0">
          {pct !== null && `${pct}%`}
          {activity.speed_bps > 0 && ` · ${bytes(activity.speed_bps)}/s`}
          {left !== null && left > 0 && ` · ${t('{time} left', { time: duration(left) })}`}
        </span>
      </div>
      {pct !== null && <Progress value={pct} className="h-1.5" />}
    </div>
  )
}
