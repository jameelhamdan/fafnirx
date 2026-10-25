import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Pause, Play, RefreshCw, RotateCcw, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader, StatusDot } from '@/components/common'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, dateTime } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import type { QueueJob, QueueState } from '@/lib/types'

/** What each kind of work is, in words a person knows. */
const KIND: Record<string, string> = {
  meta: msg('Reading details'),
  thumb: msg('Making pictures'),
  proxy: msg('Playable copies'),
  segment: msg('Video segments'),
  subtitles: msg('Subtitles'),
}

const WHAT: Record<string, string> = {
  meta: msg('Size, date, place and camera data of photos and videos.'),
  thumb: msg('The small and large pictures of photos and videos, from the artwork next to a video or a frame of it.'),
  proxy: msg('Copies of videos that browsers cannot play as they are. Slow; they wait behind everything else.'),
}

const STATE: Record<string, string> = {
  queued: msg('Waiting'),
  running: msg('Running'),
  failed: msg('Failed'),
  done: msg('Done'),
}

function Counts({ queued, running, failed, done }: { queued: number; running: number; failed: number; done: number }) {
  const t = useT()
  const parts: [number, string, string][] = [
    [running, t('running'), ''],
    [queued, t('waiting'), ''],
    [failed, t('failed'), failed > 0 ? 'text-destructive' : ''],
    [done, t('done today'), 'text-muted-foreground'],
  ]
  return (
    <p className="flex flex-wrap gap-x-4 gap-y-1 text-sm tabular-nums">
      {parts.map(([n, label, style]) => <span key={label} className={style}><b className="font-semibold">{n}</b> {label}</span>)}
    </p>
  )
}

function JobRow({ job, canControl, onCancel, onRetry }: { job: QueueJob; canControl: boolean; onCancel: (id: number) => void; onRetry: (id: number) => void }) {
  const t = useT()
  const now = useNow()
  const when = job.status === 'running' ? job.started_at : job.status === 'queued' ? job.created_at : job.finished_at
  return (
    <TableRow>
      <TableCell className="whitespace-nowrap"><StatusDot status={job.status} label={t(STATE[job.status] ?? job.status)} /></TableCell>
      <TableCell className="whitespace-nowrap">{t(KIND[job.kind] ?? job.kind)}</TableCell>
      <TableCell className="max-w-0 w-full">
        <p className="truncate" title={`${job.device}/${job.path}`}>{job.path || '—'}</p>
        {job.error && <p className="truncate text-xs text-destructive" title={job.error}>{job.error}</p>}
        {job.retry_at && <p className="text-xs text-muted-foreground">{t('Failed {n} time(s); tries again {when}', { n: job.attempts, when: dateTime(job.retry_at) })}</p>}
      </TableCell>
      <TableCell className="whitespace-nowrap text-muted-foreground">{ago(when, now)}</TableCell>
      <TableCell className="whitespace-nowrap text-right">
        {canControl && job.status === 'failed' && <Button size="sm" variant="ghost" onClick={() => onRetry(job.id)}><RotateCcw className="size-4" />{t('Retry')}</Button>}
        {canControl && (job.status === 'failed' || job.status === 'queued') && (
          <Button size="sm" variant="ghost" aria-label={t('Remove')} onClick={() => onCancel(job.id)}><Trash2 className="size-4" /></Button>
        )}
      </TableCell>
    </TableRow>
  )
}

/** The streaming service's own work: segments being cut for a video that is playing, subtitle tracks being converted. */
function StreamWork({ report }: { report: NonNullable<QueueState['stream']> }) {
  const t = useT()
  const now = useNow()
  return (
    <Card>
      <CardHeader><CardTitle>{t('Streaming')}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          {t('Made as videos are watched, {slots} at a time. {made} made since it started.', { slots: report.slots, made: report.made })}
        </p>
        {report.active.length === 0 && report.failed.length === 0 && <p className="text-sm text-muted-foreground">{t('Nothing is being made now.')}</p>}
        <ul className="divide-y text-sm">
          {report.active.map((a, i) => (
            <li key={i} className="flex items-center justify-between gap-3 py-1.5">
              <span className="min-w-0 truncate"><Badge variant="secondary" className="mr-2">{t(KIND[a.kind] ?? a.kind)}</Badge>{a.video}</span>
              <span className="shrink-0 text-muted-foreground">{a.running ? t('Running') : t('Waiting')} · {ago(a.since, now)}</span>
            </li>
          ))}
          {report.failed.map((f, i) => (
            <li key={`f${i}`} className="py-1.5 text-destructive">
              <span className="mr-2 font-medium">{t(KIND[f.kind] ?? f.kind)}</span>{f.video}
              <span className="block truncate text-xs" title={f.error}>{f.error} · {ago(f.at, now)}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}

export default function Queue() {
  const t = useT()
  const client = useQueryClient()
  const { data, error } = useQuery({ queryKey: ['queue'], queryFn: () => api.get<QueueState>('/api/queue'), refetchInterval: 3000 })
  const refresh = () => client.invalidateQueries({ queryKey: ['queue'] })
  const act = useMutation({
    mutationFn: ({ url, body }: { url: string; body?: unknown; message?: (changed: number) => string }) => api.post<{ changed: number } | null>(url, body),
    onSuccess: (r, v) => {
      if (v.message && r) toast.success(v.message(r.changed))
      void refresh()
    },
    onError: (e) => toast.error(e.message),
  })
  const retry = (id?: number) => act.mutate({ url: '/api/queue/retry', body: { id }, message: (n) => t('{n} job(s) will run again', { n }) })
  const cancel = (status: 'failed' | 'queued', id?: number) => act.mutate({ url: '/api/queue/cancel', body: { status, id } })
  const failed = data?.summary.reduce((n, s) => n + s.failed, 0) ?? 0
  const waiting = data?.summary.reduce((n, s) => n + s.queued, 0) ?? 0

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('Processing')}
        description={t('The work done in the background after files arrive: reading details, making pictures and playable copies, and the streaming service’s segments and subtitles.')}
        actions={data?.can_control ? (
          <>
            <Button variant="outline" onClick={() => act.mutate({ url: '/api/queue/pause', body: { paused: !data.paused } })}>
              {data.paused ? <Play className="size-4" /> : <Pause className="size-4" />}{data.paused ? t('Resume') : t('Pause')}
            </Button>
            <Button variant="outline" disabled={failed === 0} onClick={() => retry()}><RotateCcw className="size-4" />{t('Retry failed')}</Button>
            <Button variant="outline" disabled={failed === 0} onClick={() => cancel('failed')}><Trash2 className="size-4" />{t('Clear failed')}</Button>
            <Button variant="outline" disabled={waiting === 0} onClick={() => cancel('queued')}>{t('Cancel waiting')}</Button>
            <Button
              variant="outline"
              onClick={() => act.mutate({ url: '/api/queue/rebuild', body: { everything: false }, message: (n) => t('{n} file(s) queued', { n }) })}
            >
              <RefreshCw className="size-4" />{t('Process missing')}
            </Button>
          </>
        ) : undefined}
      />
      {error && <ErrorNote error={error} />}
      {!data && !error && <Skeleton className="h-48" />}
      {data && (
        <>
          {data.paused && <p role="status" className="rounded-lg border border-warning/40 bg-warning/10 p-3 text-sm">{t('Paused: nothing new is started. What is running now finishes.')}</p>}
          {!data.ffmpeg && <p role="status" className="rounded-lg border border-warning/40 bg-warning/10 p-3 text-sm">{t('ffmpeg is not installed on the master, so videos get no details, pictures or playable copies.')}</p>}
          <div className="grid gap-4 md:grid-cols-3">
            {['meta', 'thumb', 'proxy'].map((kind) => {
              const s = data.summary.find((x) => x.kind === kind) ?? { kind, queued: 0, running: 0, failed: 0, done: 0 }
              return (
                <Card key={kind}>
                  <CardHeader><CardTitle>{t(KIND[kind])}</CardTitle></CardHeader>
                  <CardContent className="space-y-2">
                    <Counts {...s} />
                    <p className="text-xs text-muted-foreground">{t(WHAT[kind])}</p>
                  </CardContent>
                </Card>
              )
            })}
          </div>
          {data.jobs.length === 0 ? (
            <Empty title={t('Nothing in the queue')}>{t('Files that arrive are queued here, and stay in the list for a day after they are done.')}</Empty>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('State')}</TableHead>
                  <TableHead>{t('Work')}</TableHead>
                  <TableHead>{t('File')}</TableHead>
                  <TableHead>{t('Since')}</TableHead>
                  <TableHead />
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.jobs.map((j) => (
                  <JobRow key={j.id} job={j} canControl={data.can_control} onRetry={retry} onCancel={(id) => cancel(j.status === 'failed' ? 'failed' : 'queued', id)} />
                ))}
              </TableBody>
            </Table>
          )}
          {data.stream && <StreamWork report={data.stream} />}
        </>
      )}
    </div>
  )
}
