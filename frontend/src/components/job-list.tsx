import { useMutation, useQueryClient } from '@tanstack/react-query'
import { Pause, Play, RefreshCw, Trash2 } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { ago, duration } from '@/lib/format'
import type { JobState } from '@/lib/types'
import { useNow } from '@/hooks/use-now'
import { Confirm } from './confirm'
import { Empty, StatusDot } from './common'
import { Button } from './ui/button'
import { Card, CardContent, CardHeader, CardTitle } from './ui/card'
import { useT } from '@/lib/i18n'

/** Sources and mirrors look alike: a status, the last message, and run / pause / delete. */
export function JobList({
  kind,
  jobs,
  summary,
  extraActions,
  emptyTitle,
  emptyText,
}: {
  kind: 'sources' | 'mirrors'
  jobs: JobState[]
  summary: (j: JobState) => ReactNode
  extraActions?: (j: JobState) => ReactNode
  emptyTitle: string
  emptyText: string
}) {
  const client = useQueryClient()
  const now = useNow()
  const t = useT()
  const [removing, setRemoving] = useState<JobState | null>(null)
  const refresh = () => client.invalidateQueries({ queryKey: [kind] })
  const act = useMutation({
    mutationFn: ({ job, what }: { job: JobState; what: 'run' | 'toggle' }) => api.post(`/api/${kind}/${job.key}/${what}`),
    onSuccess: (_d, v) => {
      if (v.what === 'run') toast.success(t('Run requested for {name}. It starts within a few seconds.', { name: v.job.key }))
      void refresh()
    },
    onError: (e) => toast.error(e.message),
  })
  const remove = useMutation({
    mutationFn: (job: JobState) => api.delete(`/api/${kind}/${job.key}`),
    onSuccess: (_d, job) => { toast.success(t('{name} removed', { name: job.key })); void refresh() },
    onError: (e) => toast.error(e.message),
  })

  if (jobs.length === 0) return <Empty title={emptyTitle}>{emptyText}</Empty>
  return (
    <div className="space-y-4">
      {jobs.map((j) => (
        <Card key={j.key}>
          <CardHeader className="flex-row items-start justify-between gap-2">
            <div className="min-w-0">
              <CardTitle className="truncate">{j.key}</CardTitle>
              <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-muted-foreground">
                <StatusDot status={j.enabled ? j.status : 'paused'} label={j.running ? t('Running') : j.enabled ? undefined : t('Paused')} />
                <span>{j.running ? t('started') : t('last run')} {ago(j.last_run_at, now)}</span>
                <span>{t('every {time}', { time: duration(j.interval_secs) })}</span>
              </div>
            </div>
            <div className="flex shrink-0 gap-1">
              <Button variant="outline" size="sm" disabled={j.running} onClick={() => act.mutate({ job: j, what: 'run' })}><Play className="size-4" />{t('Run now')}</Button>
              <Button variant="outline" size="icon" className="size-8" aria-label={j.enabled ? t('Pause') : t('Resume')} onClick={() => act.mutate({ job: j, what: 'toggle' })}>
                {j.enabled ? <Pause className="size-4" /> : <RefreshCw className="size-4" />}
              </Button>
              {extraActions?.(j)}
              <Button variant="outline" size="icon" className="size-8" aria-label={t('Delete')} onClick={() => setRemoving(j)}><Trash2 className="size-4" /></Button>
            </div>
          </CardHeader>
          <CardContent className="space-y-2 text-sm">
            <div className="text-muted-foreground">{summary(j)}</div>
            {j.message && <pre className="overflow-x-auto rounded-md bg-muted p-3 text-xs whitespace-pre-wrap">{j.message}</pre>}
          </CardContent>
        </Card>
      ))}
      <Confirm
        open={removing !== null}
        title={t('Remove {name}?', { name: removing?.key ?? '' })}
        description={kind === 'sources' ? t('Its files are kept for the retention period, then removed.') : t('Data already on the remote is not touched.')}
        action={t('Remove')}
        destructive
        onCancel={() => setRemoving(null)}
        onConfirm={() => { if (removing) remove.mutate(removing); setRemoving(null) }}
      />
    </div>
  )
}
