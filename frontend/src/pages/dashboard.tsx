import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { STATUS } from '@/lib/status'
import { ActivityView, Empty, ErrorNote, KindBadge, PageHeader, StatusDot } from '@/components/common'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Progress } from '@/components/ui/progress'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { locale, msg, useT } from '@/lib/i18n'
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
}

const ACTION: Record<string, string> = { add: msg('add'), update: msg('update'), delete: msg('delete'), rename: msg('rename') }

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

/** Changes per day for two weeks, stacked: added, updated, deleted. */
function ActivityChart({ days }: { days: DashboardData['activity'] }) {
  const t = useT()
  const peak = Math.max(1, ...days.map((d) => d.added + d.updated + d.deleted))
  return (
    <div>
      <div className="flex h-32 items-end gap-1.5" role="img" aria-label={t('Changes per day over the last two weeks')}>
        {days.map((d) => {
          const total = d.added + d.updated + d.deleted
          const label = new Date(d.day * 86400_000).toLocaleDateString(locale(), { month: 'short', day: 'numeric', timeZone: 'UTC' })
          return (
            <div key={d.day} className="group relative flex h-full flex-1 flex-col justify-end" title={`${label}: ${t('{added} added, {updated} updated, {deleted} deleted', { added: d.added, updated: d.updated, deleted: d.deleted })}`}>
              {total === 0 ? (
                <div className="h-px bg-border" />
              ) : (
                <div className="flex flex-col-reverse overflow-hidden rounded-sm" style={{ height: `${(100 * total) / peak}%` }}>
                  <div className="bg-primary" style={{ flex: d.added }} />
                  <div className="bg-primary/55" style={{ flex: d.updated }} />
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
          <span className="flex items-center gap-1"><i className="size-2 rounded-sm bg-destructive/70" />{t('Deleted')}</span>
        </span>
        <span>{t('Today')}</span>
      </div>
    </div>
  )
}

export default function Dashboard() {
  const t = useT()
  const client = useQueryClient()
  const now = useNow()
  const { data, error } = useQuery({ queryKey: ['dashboard'], queryFn: () => api.get<DashboardData>('/api/dashboard') })
  const dismiss = useMutation({
    mutationFn: (id: number) => api.post(`/api/alerts/${id}/dismiss`),
    onSuccess: () => client.invalidateQueries({ queryKey: ['dashboard'] }),
    onError: (e) => toast.error(e.message),
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
                      <Link to={d.kind === 'device' ? `/devices/${encodeURIComponent(d.folder)}` : `/files?device=${encodeURIComponent(d.folder)}`} className="font-medium hover:underline">{d.folder}</Link>
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
          {data.recent.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t('Nothing has been collected yet.')}</p>
          ) : (
            <ul className="divide-y">
              {data.recent.map((e) => (
                <li key={e.id} className="flex items-center gap-3 py-2 text-sm">
                  <span className={cn('w-16 shrink-0 text-xs font-medium uppercase', e.action === 'delete' ? 'text-destructive' : e.action === 'update' ? 'text-warning' : 'text-success')}>{t(ACTION[e.action] ?? e.action)}</span>
                  <Link to={`/files?device=${encodeURIComponent(e.device)}&file=${encodeURIComponent(e.path)}`} className="min-w-0 flex-1 truncate hover:underline" title={e.path}>
                    <span className="text-muted-foreground">{e.device}/</span>{e.path}
                  </Link>
                  <span className="shrink-0 text-xs text-muted-foreground">{ago(e.ts, now)}</span>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
