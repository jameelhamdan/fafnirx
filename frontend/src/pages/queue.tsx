import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
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

/** What each kind of work is, in words a person knows. A kind a module brings that is not here is shown as the module names it. */
const KIND: Record<string, string> = {
  meta: msg('Reading details'),
  thumb: msg('Making pictures'),
  source: msg('Reading a source'),
  mirror: msg('Copying offsite'),
  list: msg('Listing a source'),
  fetch: msg('Fetching a file'),
  segment: msg('Video segments'),
  subtitles: msg('Subtitles'),
}

const WHAT: Record<string, string> = {
  meta: msg('Size, date, place and camera data of photos and videos.'),
  thumb: msg('The small and large pictures of photos and videos, from the artwork next to a video or a frame of it.'),
  source: msg('A source reads its storage on a schedule and writes what is new into its folder.'),
  mirror: msg('A mirror copies the folders of the master to its offsite storage.'),
}

/** The cards at the top: the master's own kinds of work first, then any other kind that has work to show. */
const CARDS = ['meta', 'thumb', 'source', 'mirror']

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

function JobRow({ job, canControl, onCancel, onRetry }: { job: QueueJob; canControl: boolean; onCancel: (id: string) => void; onRetry: (id: string) => void }) {
  const t = useT()
  const now = useNow()
  const when = job.status === 'running' ? job.started_at : job.status === 'queued' ? job.created_at : job.finished_at
  // Only the jobs in the database can be steered: the rest are runs of sources and mirrors and what modules report.
  const steerable = job.kind === 'meta' || job.kind === 'thumb'
  return (
    <TableRow>
      <TableCell className="whitespace-nowrap"><StatusDot status={job.status} label={t(STATE[job.status] ?? job.status)} /></TableCell>
      <TableCell className="whitespace-nowrap">
        {t(KIND[job.kind] ?? job.kind)}
        {job.plugin && <Badge variant="outline" className="ms-2">{job.plugin}</Badge>}
      </TableCell>
      <TableCell className="max-w-0 w-full">
        <p className="truncate" title={`${job.device}/${job.path}`}>{job.path || '—'}</p>
        {job.device && !steerable && <p className="truncate text-xs text-muted-foreground">{job.device}</p>}
        {job.error && <p className="truncate text-xs text-destructive" title={job.error}>{job.error}</p>}
        {job.note && <p className="truncate text-xs text-muted-foreground" title={job.note}>{job.note}</p>}
        {job.retry_at && <p className="text-xs text-muted-foreground">{t('Failed {n} time(s); tries again {when}', { n: job.attempts, when: dateTime(job.retry_at) })}</p>}
      </TableCell>
      <TableCell className="whitespace-nowrap text-muted-foreground">{ago(when, now)}</TableCell>
      <TableCell className="whitespace-nowrap text-right">
        {canControl && steerable && job.status === 'failed' && <Button size="sm" variant="ghost" onClick={() => onRetry(job.id)}><RotateCcw className="size-4" />{t('Retry')}</Button>}
        {canControl && steerable && (job.status === 'failed' || job.status === 'queued') && (
          <Button size="sm" variant="ghost" aria-label={t('Remove')} onClick={() => onCancel(job.id)}><Trash2 className="size-4" /></Button>
        )}
      </TableCell>
    </TableRow>
  )
}

export default function Queue() {
  const t = useT()
  const client = useQueryClient()
  const { data, error } = useQuery({ queryKey: ['queue'], queryFn: () => api.get<QueueState>('/api/queue'), refetchInterval: 3000 })
  const refresh = () => client.invalidateQueries({ queryKey: ['queue'] })
  const act = useApiMutation(({ url, body }: { url: string; body?: unknown; message?: (changed: number) => string }) => api.post<{ changed: number } | null>(url, body), {
    onSuccess: (r, v) => {
      if (v.message && r) toast.success(v.message(r.changed))
      void refresh()
    },
    toastError: true,
  })
  const retry = (id?: string) => act.mutate({ url: '/api/queue/retry', body: { id }, message: (n) => t('{n} job(s) will run again', { n }) })
  const cancel = (status: 'failed' | 'queued', id?: string) => act.mutate({ url: '/api/queue/cancel', body: { status, id } })
  const steered = (kind: string) => kind === 'meta' || kind === 'thumb'
  const failed = data?.summary.filter((s) => steered(s.kind)).reduce((n, s) => n + s.failed, 0) ?? 0
  const waiting = data?.summary.filter((s) => steered(s.kind)).reduce((n, s) => n + s.queued, 0) ?? 0
  const cards = data ? [...CARDS.slice(0, 2), ...CARDS.slice(2).filter((k) => data.summary.some((s) => s.kind === k)), ...data.summary.map((s) => s.kind).filter((k) => !CARDS.includes(k))] : []

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('Processing')}
        description={t('All the work done in the background: reading details, making pictures, reading sources, copying to mirrors, and what the modules that are set up are working on.')}
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
            <Button
              variant="outline"
              onClick={() => act.mutate({ url: '/api/queue/rebuild', body: { everything: true }, message: (n) => t('{n} file(s) queued', { n }) })}
            >
              <RefreshCw className="size-4" />{t('Read everything again')}
            </Button>
          </>
        ) : undefined}
      />
      {error && <ErrorNote error={error} />}
      {!data && !error && <Skeleton className="h-48" />}
      {data && (
        <>
          {data.paused && <p role="status" className="rounded-lg border border-warning/40 bg-warning/10 p-3 text-sm">{t('Paused: nothing new is started. What is running now finishes.')}</p>}
          <div className="grid gap-4 md:grid-cols-3">
            {cards.map((kind) => {
              const s = data.summary.find((x) => x.kind === kind) ?? { kind, queued: 0, running: 0, failed: 0, done: 0 }
              return (
                <Card key={kind}>
                  <CardHeader><CardTitle>{t(KIND[kind] ?? kind)}</CardTitle></CardHeader>
                  <CardContent className="space-y-2">
                    <Counts {...s} />
                    {WHAT[kind] && <p className="text-xs text-muted-foreground">{t(WHAT[kind])}</p>}
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
                  <TableHead>{t('For')}</TableHead>
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
        </>
      )}
    </div>
  )
}
