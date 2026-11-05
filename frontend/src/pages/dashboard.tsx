import { useApiMutation } from '@/lib/mutation'
import { EventList } from '@/components/event-list'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { Link } from 'react-router-dom'
import { STATUS } from '@/lib/status'
import { ActivityView, Empty, ErrorNote, KindBadge, PageHeader, StatusDot } from '@/components/common'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { locale, msg, useT } from '@/lib/i18n'
import { filesLink } from '@/lib/links'
import { ago, bytes, count } from '@/lib/format'
import type { Dashboard as DashboardData } from '@/lib/types'
import { cn } from '@/lib/utils'

const ALERT: Record<string, string> = {
  device_offline: msg('Device offline'),
  upload_error: msg('Upload error'),
  disk_usage: msg('Disk usage'),
  source_failed: msg('Source failed'),
  mirror_failed: msg('Mirror failed'),
  device_conflict: msg('Device used by two computers'),
  deletions_held: msg('Deletions held back'),
  misplaced_files: msg('Files outside a device folder'),
}


function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <Card className="gap-1 py-4">
      <CardHeader className="pb-0">
        <CardDescription>{label}</CardDescription>
        <CardTitle className="text-2xl tabular-nums">{value}</CardTitle>
      </CardHeader>
      {hint && <CardContent className="text-xs text-muted-foreground">{hint}</CardContent>}
    </Card>
  )
}

/** Changes per day for two weeks, stacked: added, updated, renamed, deleted. */
function ActivityChart({ days }: { days: DashboardData['activity'] }) {
  const t = useT()
  const peak = Math.max(1, ...days.map((d) => d.added + d.updated + d.renamed + d.deleted))
  return (
    <div>
      <div className="flex h-32 items-end gap-1.5" role="img" aria-label={t('Changes per day over the last two weeks')}>
        {days.map((d) => {
          const total = d.added + d.updated + d.renamed + d.deleted
          const label = new Date(d.day * 86400_000).toLocaleDateString(locale(), { month: 'short', day: 'numeric', timeZone: 'UTC' })
          return (
            <div key={d.day} className="group relative flex h-full flex-1 flex-col justify-end" title={`${label}: ${t('{added} added, {updated} updated, {renamed} renamed, {deleted} deleted', { added: d.added, updated: d.updated, renamed: d.renamed, deleted: d.deleted })}`}>
              {total === 0 ? (
                <div className="h-px bg-border" />
              ) : (
                <div className="flex flex-col-reverse overflow-hidden rounded-sm" style={{ height: `${(100 * total) / peak}%` }}>
                  <div className="bg-primary" style={{ flex: d.added }} />
                  <div className="bg-primary/55" style={{ flex: d.updated }} />
                  <div className="bg-foreground/35" style={{ flex: d.renamed }} />
                  <div className="bg-destructive/70" style={{ flex: d.deleted }} />
                </div>
              )}
            </div>
          )
        })}
      </div>
      <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
        <span>{new Date(days[0].day * 86400_000).toLocaleDateString(locale(), { month: 'short', day: 'numeric', timeZone: 'UTC' })}</span>
        <span className="flex gap-3">
          <span className="flex items-center gap-1"><i className="size-2 rounded-sm bg-primary" />{t('Added')}</span>
          <span className="flex items-center gap-1"><i className="size-2 rounded-sm bg-primary/55" />{t('Updated')}</span>
          <span className="flex items-center gap-1"><i className="size-2 rounded-sm bg-foreground/35" />{t('Renamed')}</span>
          <span className="flex items-center gap-1"><i className="size-2 rounded-sm bg-destructive/70" />{t('Deleted')}</span>
        </span>
        <span>{t('Today')}</span>
      </div>
    </div>
  )
}

/** One colour for each of the largest devices; what is left is grey. */
const SLICE = [150, 250, 60, 310, 200, 20].map((hue) => `oklch(0.7 0.15 ${hue})`)

/** Where the stored bytes are: a pie of the devices (the largest ones, the rest together) and what each holds. */
function StorageByDevice({ devices }: { devices: DashboardData['devices'] }) {
  const t = useT()
  const sorted = devices.filter((d) => d.bytes > 0).sort((a, b) => b.bytes - a.bytes)
  const total = sorted.reduce((n, d) => n + d.bytes, 0)
  if (total === 0) return <p className="text-sm text-muted-foreground">{t('Nothing stored yet.')}</p>
  const rest = sorted.slice(SLICE.length).reduce((n, d) => n + d.bytes, 0)
  const slices = [
    ...sorted.slice(0, SLICE.length).map((d, i) => ({ label: d.folder, bytes: d.bytes, color: SLICE[i] })),
    ...(rest > 0 ? [{ label: t('Other'), bytes: rest, color: 'var(--muted-foreground)' }] : []),
  ]
  let angle = -Math.PI / 2
  const wedges = slices.map((slice) => {
    const share = (100 * slice.bytes) / total
    const from = angle
    angle += (2 * Math.PI * slice.bytes) / total
    const large = angle - from > Math.PI ? 1 : 0
    const d = `M0 0 L${Math.cos(from)} ${Math.sin(from)} A1 1 0 ${large} 1 ${Math.cos(angle)} ${Math.sin(angle)} Z`
    return { ...slice, share, d, text: `${slice.label}: ${bytes(slice.bytes)} (${share < 1 ? '<1' : Math.round(share)}%)` }
  })
  return (
    <div className="flex items-center gap-4">
      <svg viewBox="-1.02 -1.02 2.04 2.04" className="size-32 shrink-0" role="img" aria-label={t('Share of the stored bytes by device')}>
        {wedges.length === 1 ? (
          <circle r="1" fill={wedges[0].color}><title>{wedges[0].text}</title></circle>
        ) : (
          wedges.map((w) => (
            <path key={w.label} d={w.d} fill={w.color} className="stroke-card" strokeWidth="0.02" strokeLinejoin="round"><title>{w.text}</title></path>
          ))
        )}
      </svg>
      <ul className="min-w-0 flex-1 space-y-1.5 text-sm">
        {wedges.map((w) => (
          <li key={w.label} className="flex items-center gap-2" title={w.text}>
            <i className="size-2.5 shrink-0 rounded-sm" style={{ background: w.color }} />
            <span className="min-w-0 flex-1 truncate">{w.label}</span>
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{bytes(w.bytes)} · {w.share < 1 ? '<1' : Math.round(w.share)}%</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export default function Dashboard() {
  const t = useT()
  const client = useQueryClient()
  const now = useNow()
  const { data, error } = useQuery({ queryKey: ['dashboard'], queryFn: () => api.get<DashboardData>('/api/dashboard') })
  const dismiss = useApiMutation((id: string) => api.post(`/api/alerts/${id}/dismiss`), {
    onSuccess: () => client.invalidateQueries({ queryKey: ['dashboard'] }),
    toastError: true,
  })

  if (error) return <ErrorNote error={error} />
  if (!data) {
    return (
      <div className="space-y-4">
        <Skeleton className="h-9 w-48" />
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {[0, 1, 2, 3].map((i) => <Skeleton key={i} className="h-24" />)}
        </div>
        <Skeleton className="h-64" />
      </div>
    )
  }
  const clients = data.devices.filter((d) => d.kind === 'device')
  const jobs = [...data.sources.map((s) => ({ ...s, type: 'source' })), ...data.mirrors.map((m) => ({ ...m, type: 'mirror' }))]

  return (
    <div>
      <PageHeader title={t('Dashboard')} description={t('What the master holds, who is sending it, and anything that needs attention.')} />

      {data.alerts.length > 0 && (
        <div className="mb-6 space-y-2">
          {data.alerts.map((a) => (
            <div key={a.id} className="flex items-start gap-3 rounded-lg border border-warning/50 bg-warning/10 p-3 text-sm">
              <div className="min-w-0 flex-1">
                <p className="font-medium">{a.key ? `${t(ALERT[a.kind] ?? a.kind.replace('_', ' '))}: ${a.key}` : t(ALERT[a.kind] ?? a.kind.replace('_', ' '))}</p>
                <p className="whitespace-pre-line break-words text-muted-foreground">{a.message}</p>
              </div>
              <Button variant="ghost" size="icon" className="size-7" onClick={() => dismiss.mutate(a.id)} aria-label={t('Dismiss alert')}>
                <X className="size-4" />
              </Button>
            </div>
          ))}
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <Stat label={t('Devices online')} value={`${data.online} / ${clients.length}`} hint={clients.length === 0 ? t('Add a device to start collecting') : undefined} />
        <Stat label={t('Files')} value={count(data.totals.files)} hint={data.totals.marked_deleted ? t('{count} deleted, waiting for retention', { count: count(data.totals.marked_deleted) }) : undefined} />
        <Stat label={t('Stored')} value={bytes(data.totals.bytes)} />
        <Card className="gap-1 py-4">
          <CardHeader className="pb-0">
            <CardDescription>{t('Master disk')}</CardDescription>
            <CardTitle className="text-2xl tabular-nums">{data.disk ? `${data.disk.percent}%` : '—'}</CardTitle>
          </CardHeader>
          <CardContent>
            {data.disk && (
              <>
                <Progress value={data.disk.percent} className={cn('h-1.5', data.disk.percent >= 80 && '[&>div]:bg-warning')} />
                <p className="mt-1.5 text-xs text-muted-foreground">{t('{free} free of {total}', { free: bytes(data.disk.free), total: bytes(data.disk.total) })}</p>
              </>
            )}
          </CardContent>
        </Card>
      </div>

      <div className="mt-6 grid gap-6 lg:grid-cols-5">
        <Card className="lg:col-span-3">
          <CardHeader>
            <CardTitle>{t('Devices')}</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            {data.devices.length === 0 ? (
              <Empty title={t('No devices yet')}>{t('Add one on the Devices page, or create a local folder on the Files page.')}</Empty>
            ) : (
              data.devices.map((d) => (
                <div key={d.folder} className="space-y-2">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <Link to={d.kind === 'device' ? `/devices/${encodeURIComponent(d.folder)}` : filesLink(d.folder)} className="font-medium hover:underline">{d.folder}</Link>
                      <KindBadge kind={d.kind} />
                    </div>
                    <div className="flex items-center gap-4 text-xs text-muted-foreground">
                      <span>{t('{files} files', { files: count(d.files) })} · {bytes(d.bytes)}</span>
                      {d.kind === 'device' && <StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? t('Revoked') : d.online ? t('Online') : t('Seen {when}', { when: ago(d.last_seen, now) })} />}
                    </div>
                  </div>
                  {d.activity && <ActivityView activity={d.activity} />}
                </div>
              ))
            )}
          </CardContent>
        </Card>

        <div className="space-y-6 lg:col-span-2">
          <Card>
            <CardHeader>
              <CardTitle>{t('Storage by device')}</CardTitle>
            </CardHeader>
            <CardContent>
              <StorageByDevice devices={data.devices} />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>{t('Changes, last 14 days')}</CardTitle>
            </CardHeader>
            <CardContent>
              <ActivityChart days={data.activity} />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>{t('Sources and mirrors')}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {jobs.length === 0 ? (
                <p className="text-sm text-muted-foreground">{t('None configured.')}</p>
              ) : (
                jobs.map((j) => (
                  <div key={j.type + j.key} className="flex items-center justify-between gap-2 text-sm">
                    <Link to={j.type === 'source' ? '/sources' : '/mirrors'} className="truncate hover:underline">{j.key}</Link>
                    <StatusDot status={j.enabled ? j.status : 'paused'} label={j.running ? t('Running') : j.enabled ? `${t(STATUS[j.status] ?? j.status)} · ${ago(j.last_run_at, now)}` : t('Paused')} />
                  </div>
                ))
              )}
            </CardContent>
          </Card>
        </div>
      </div>

      <Card className="mt-6">
        <CardHeader>
          <CardTitle>{t('Recent activity')}</CardTitle>
        </CardHeader>
        <CardContent>
          <EventList events={data.recent} empty={t('Nothing has been collected yet.')} showFolder />
        </CardContent>
      </Card>
    </div>
  )
}
