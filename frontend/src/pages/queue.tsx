import { useQueryClient } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { Pause, Play, RefreshCw, RotateCcw, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Picker, Empty, ErrorNote, PageHeader, StatusDot, Callout, Panel } from '@/components/common'
import { DataTable, SelectAll, SelectRow, TableToolbar } from '@/components/data-table'
import { matches, useSelection, useTableView, type Selection, type TableView } from '@/lib/table-view'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useNow } from '@/hooks/use-now'
import { cancelJobs, pauseQueue, rebuildPictures, retryJobs, useQueue } from '@/services/jobs'
import { KEYS } from '@/services/keys'
import { ago, dateTime } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import type { QueueJob } from '@/lib/types'

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

/** The kinds at the top: the master's own kinds of work first, then any other kind that has work to show. */
const CARDS = ['meta', 'thumb', 'source', 'mirror']

const STATE: Record<string, string> = {
  queued: msg('Waiting'),
  running: msg('Running'),
  failed: msg('Failed'),
  done: msg('Done'),
}

/** Only the jobs in the database can be steered: the rest are runs of sources and mirrors and what modules report. */
const steered = (kind: string) => kind === 'meta' || kind === 'thumb'

function Counts({ queued, running, failed, done }: { queued: number; running: number; failed: number; done: number }) {
  const t = useT()
  const parts: [number, string, string][] = [
    [running, t('running'), running > 0 ? 'text-primary-ink' : 'text-muted-foreground'],
    [queued, t('waiting'), queued > 0 ? '' : 'text-muted-foreground'],
    [failed, t('failed'), failed > 0 ? 'text-destructive' : 'text-muted-foreground'],
    [done, t('done today'), 'text-muted-foreground'],
  ]
  return (
    <p className="num flex flex-wrap gap-x-3 gap-y-0.5 text-compact">
      {parts.map(([n, label, style]) => <span key={label} className={style}><b className="font-medium">{n}</b> <span className="text-2xs">{label}</span></span>)}
    </p>
  )
}

function JobRow({ job, canControl, view, selection, onCancel, onRetry }: { job: QueueJob; canControl: boolean; view: TableView; selection: Selection; onCancel: (id: string) => void; onRetry: (id: string) => void }) {
  const t = useT()
  const now = useNow()
  const when = job.status === 'running' ? job.started_at : job.status === 'queued' ? job.created_at : job.finished_at
  const canSteer = steered(job.kind)
  return (
    <TableRow data-state={selection.selected.has(job.id) ? 'selected' : undefined}>
      {canControl && <TableCell className="w-8">{canSteer ? <SelectRow selection={selection} id={job.id} /> : null}</TableCell>}
      <TableCell className="whitespace-nowrap"><StatusDot status={job.status} label={t(STATE[job.status] ?? job.status)} /></TableCell>
      {view.show('work') && (
        <TableCell className="whitespace-nowrap">
          {t(KIND[job.kind] ?? job.kind)}
          {job.plugin && <Badge variant="outline" className="ms-2">{job.plugin}</Badge>}
        </TableCell>
      )}
      <TableCell className="max-w-0 w-full">
        <p className="num truncate text-xs" title={`${job.folder}/${job.path}`}>{job.path || '—'}</p>
        {job.folder && !canSteer && <p className="num truncate text-2xs text-muted-foreground">{job.folder}</p>}
        {job.error && <p className="truncate text-xs text-destructive" title={job.error}>{job.error}</p>}
        {job.note && <p className="truncate text-xs text-muted-foreground" title={job.note}>{job.note}</p>}
        {job.retry_at && <p className="text-xs text-muted-foreground">{t('Failed {n} time(s); tries again {when}', { n: job.attempts, when: dateTime(job.retry_at) })}</p>}
      </TableCell>
      {view.show('since') && <TableCell className="num whitespace-nowrap text-end text-xs text-muted-foreground">{ago(when, now)}</TableCell>}
      <TableCell className="whitespace-nowrap text-end">
        {canControl && canSteer && job.status === 'failed' && <Button size="xs" variant="ghost" onClick={() => onRetry(job.id)}><RotateCcw />{t('Retry')}</Button>}
        {canControl && canSteer && (job.status === 'failed' || job.status === 'queued') && (
          <Button size="icon-xs" variant="ghost" aria-label={t('Remove')} onClick={() => onCancel(job.id)}><Trash2 /></Button>
        )}
      </TableCell>
    </TableRow>
  )
}

export default function Queue() {
  const t = useT()
  const client = useQueryClient()
  const { data, error } = useQueue()
  const refresh = () => client.invalidateQueries({ queryKey: KEYS.queue })
  const act = useApiMutation(({ run }: { run: () => Promise<{ changed: number } | null>; message?: (changed: number) => string }) => run(), {
    onSuccess: (r, v) => {
      if (v.message && r) toast.success(v.message(r.changed))
    },
    invalidate: [KEYS.queue],
  })
  const retry = (id?: string) => act.mutate({ run: () => retryJobs(id), message: (n) => t('{n} job(s) will run again', { n }) })
  const cancel = (status: 'failed' | 'queued', id?: string) => act.mutate({ run: () => cancelJobs(status, id) })
  const failed = data?.summary.filter((s) => steered(s.kind)).reduce((n, s) => n + s.failed, 0) ?? 0
  const waiting = data?.summary.filter((s) => steered(s.kind)).reduce((n, s) => n + s.queued, 0) ?? 0
  const cards = data ? [...CARDS.slice(0, 2), ...CARDS.slice(2).filter((k) => data.summary.some((s) => s.kind === k)), ...data.summary.map((s) => s.kind).filter((k) => !CARDS.includes(k))] : []

  const [filter, setFilter] = useState('')
  const [state, setState] = useState('')
  const columns = useMemo(() => [{ id: 'state', label: t('State'), locked: true }, { id: 'work', label: t('Work') }, { id: 'for', label: t('For'), locked: true }, { id: 'since', label: t('Since') }], [t])
  const view = useTableView('queue', columns)
  const jobs = (data?.jobs ?? []).filter((j) => (state === '' || j.status === state) && matches(filter, j.path, j.folder, j.kind, t(KIND[j.kind] ?? j.kind), j.plugin, j.error))
  const selection = useSelection(useMemo(() => jobs.filter((j) => steered(j.kind)).map((j) => j.id), [jobs]))
  const picked = (data?.jobs ?? []).filter((j) => selection.selected.has(j.id))
  /** Retry or remove every ticked job that can be: one request each, the server has no batch call. */
  async function bulk(what: 'retry' | 'cancel') {
    const rows = picked.filter((j) => (what === 'retry' ? j.status === 'failed' : j.status === 'failed' || j.status === 'queued'))
    try {
      await Promise.all(rows.map((j) => (what === 'retry' ? retryJobs(j.id) : cancelJobs(j.status as 'failed' | 'queued', j.id))))
    } catch (e) {
      toast.error(e instanceof Error ? e.message : t('Something went wrong'))
    }
    selection.clear()
    void refresh()
  }

  return (
    <div className="space-y-3">
      <PageHeader
        title={t('Processing')}
        description={t('All the work done in the background: reading details, making pictures, reading sources, copying to mirrors, and what the modules that are set up are working on.')}
        actions={data?.can_control ? (
          <>
            <Button size="sm" variant="outline" onClick={() => act.mutate({ run: () => pauseQueue(!data.paused) })}>
              {data.paused ? <Play /> : <Pause />}{data.paused ? t('Resume') : t('Pause')}
            </Button>
            <Button size="sm" variant="outline" disabled={failed === 0} onClick={() => retry()}><RotateCcw />{t('Retry failed')}</Button>
            <Button size="sm" variant="outline" disabled={failed === 0} onClick={() => cancel('failed')}><Trash2 />{t('Clear failed')}</Button>
            <Button size="sm" variant="outline" disabled={waiting === 0} onClick={() => cancel('queued')}>{t('Cancel waiting')}</Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => act.mutate({ run: () => rebuildPictures(false), message: (n) => t('{n} file(s) queued', { n }) })}
            >
              <RefreshCw />{t('Process missing')}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => act.mutate({ run: () => rebuildPictures(true), message: (n) => t('{n} file(s) queued', { n }) })}
            >
              <RefreshCw />{t('Read everything again')}
            </Button>
          </>
        ) : undefined}
      />
      {error && <ErrorNote error={error} />}
      {!data && !error && <Skeleton className="h-48" />}
      {data && (
        <>
          {data.paused && <Callout role="status" className="py-1.5 text-compact">{t('Paused: nothing new is started. What is running now finishes.')}</Callout>}
          <Panel as="section" className="flex flex-wrap" aria-label={t('Work by kind')}>
            {cards.map((kind) => {
              const s = data.summary.find((x) => x.kind === kind) ?? { kind, queued: 0, running: 0, failed: 0, done: 0 }
              return (
                <div key={kind} className="min-w-52 flex-1 border-e px-4 py-3" title={WHAT[kind] ? t(WHAT[kind]) : undefined}>
                  <p className="text-xs font-medium text-muted-foreground">{t(KIND[kind] ?? kind)}</p>
                  <Counts {...s} />
                </div>
              )
            })}
          </Panel>
          {data.jobs.length === 0 ? (
            <Empty title={t('Nothing in the queue')}>{t('Files that arrive are queued here, and stay in the list for a day after they are done.')}</Empty>
          ) : (
            <DataTable
              view={view}
              toolbar={
                <TableToolbar
                  view={view}
                  filter={filter}
                  onFilter={setFilter}
                  placeholder={t('Filter by file, folder or kind')}
                  shown={jobs.length}
                  total={data.jobs.length}
                  selection={data.can_control ? selection : undefined}
                  actions={
                    <>
                      <Button size="xs" variant="outline" disabled={!picked.some((j) => j.status === 'failed')} onClick={() => void bulk('retry')}><RotateCcw />{t('Retry')}</Button>
                      <Button size="xs" variant="outline" disabled={!picked.some((j) => j.status === 'failed' || j.status === 'queued')} onClick={() => void bulk('cancel')}><Trash2 />{t('Remove')}</Button>
                    </>
                  }
                >
                  <Picker
                    size="sm"
                    className="w-40"
                    label={t('State')}
                    value={state}
                    onChange={setState}
                    options={[['', t('Any state')], ...Object.entries(STATE).map(([k, label]): [string, string] => [k, t(label)])]}
                  />
                </TableToolbar>
              }
            >
              <Table>
                <TableHeader>
                  <TableRow>
                    {data.can_control && <TableHead className="w-8"><SelectAll selection={selection} /></TableHead>}
                    <TableHead>{t('State')}</TableHead>
                    {view.show('work') && <TableHead>{t('Work')}</TableHead>}
                    <TableHead>{t('For')}</TableHead>
                    {view.show('since') && <TableHead className="text-end">{t('Since')}</TableHead>}
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {jobs.length === 0 && <TableRow className="hover:bg-transparent"><TableCell colSpan={6} className="py-6 text-center text-muted-foreground">{t('No job matches.')}</TableCell></TableRow>}
                  {jobs.map((j) => (
                    <JobRow key={j.id} job={j} view={view} canControl={data.can_control} selection={selection} onRetry={retry} onCancel={(id) => cancel(j.status === 'failed' ? 'failed' : 'queued', id)} />
                  ))}
                </TableBody>
              </Table>
            </DataTable>
          )}
        </>
      )}
    </div>
  )
}
