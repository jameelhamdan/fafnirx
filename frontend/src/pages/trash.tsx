import { useInfiniteQuery } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { File, RotateCcw, Trash2 } from 'lucide-react'
import { useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Confirm } from '@/components/confirm'
import { Thumb } from '@/components/library/thumb'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api, mediaUrl } from '@/lib/api'
import { ago, bytes, count, date, plural } from '@/lib/format'
import { useRefreshLibrary } from '@/lib/library'
import type { Trash, TrashEntry } from '@/lib/types'
import { cn } from '@/lib/utils'
import { useT } from '@/lib/i18n'

/** The most files the server removes in one call. */
const PURGE_AT_ONCE = 500

/** Deleted files of every folder. They are removed for good when the retention period ends, or at once on request (to get the space back); until then they come back with one click. */
export default function TrashPage() {
  const t = useT()
  const now = useNow()
  const refresh = useRefreshLibrary()
  const [selected, setSelected] = useState<Set<string>>(new Set())
  /** The files the person is asked to remove for good. */
  const [purging, setPurging] = useState<TrashEntry[] | null>(null)
  const trash = useInfiniteQuery({
    queryKey: ['trash'],
    initialPageParam: 0,
    queryFn: ({ pageParam }) => api.get<Trash>(`/api/trash?offset=${pageParam}`),
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.entries.length, 0)
      return loaded < last.total ? loaded : undefined
    },
  })
  const entries = useMemo(() => trash.data?.pages.flatMap((p) => p.entries) ?? [], [trash.data])
  const total = trash.data?.pages[0]?.total ?? 0
  const retention = trash.data?.pages[0]?.retention_days ?? 30
  const chosen = entries.filter((e) => selected.has(e.id))

  const restore = useApiMutation(async (list: TrashEntry[]) => {
    let done = 0
    for (const e of list) {
      await api.post('/api/files/restore', { folder: e.folder, path: e.path, version: e.version })
      done++
    }
    return done
  }, {
    onSuccess: (done) => {
      toast.success(t('Restored {files}', { files: plural(done, 'file') }))
      setSelected(new Set())
      void refresh()
    },
    onError: (e) => {
      toast.error(e.message)
      void refresh()
    },
  })
  const purge = useApiMutation(async (list: TrashEntry[]) => {
    let files = 0
    let freed = 0
    for (let i = 0; i < list.length; i += PURGE_AT_ONCE) {
      const done = await api.post<{ files: number; bytes: number }>('/api/trash/purge', { ids: list.slice(i, i + PURGE_AT_ONCE).map((e) => e.id) })
      files += done.files
      freed += done.bytes
    }
    return { files, freed }
  }, {
    onSuccess: ({ files, freed }) => {
      toast.success(t('Deleted {files} for good, {size} freed', { files: plural(files, 'file'), size: bytes(freed) }))
      setSelected(new Set())
      setPurging(null)
      void refresh()
    },
    onError: (e) => {
      toast.error(e.message)
      setPurging(null)
      void refresh()
    },
  })
  const toggle = (id: string) => setSelected((s) => { const n = new Set(s); if (!n.delete(id)) n.add(id); return n })

  return (
    <div>
      <PageHeader title={t('Trash')} description={t('Deleted files stay here for {days} days, then they are removed for good. Restoring puts a file back where it was, with its albums, tags and favorite.', { days: retention })} />
      {chosen.length > 0 && (
        <div className="mb-3 flex items-center gap-2 rounded-lg border bg-popover p-2 shadow-sm">
          <span className="px-1 text-sm font-medium">{t('{count} selected', { count: chosen.length })}</span>
          <Button size="sm" className="ml-auto" disabled={restore.isPending || purge.isPending} onClick={() => restore.mutate(chosen)}><RotateCcw className="size-4" />{t('Restore')}</Button>
          <Button size="sm" variant="destructive" disabled={restore.isPending || purge.isPending} onClick={() => setPurging(chosen)}><Trash2 className="size-4" />{t('Delete forever')}</Button>
          <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>{t('Clear')}</Button>
        </div>
      )}
      {trash.error ? <ErrorNote error={trash.error} /> : !trash.data ? (
        <Skeleton className="h-64" />
      ) : entries.length === 0 ? (
        <Empty title={t('The trash is empty')}>{t('Files you delete from the web app, or that disappear from a device, wait here before they are removed for good.')}</Empty>
      ) : (
        <>
          <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
            {entries.map((e) => {
              const on = selected.has(e.id)
              return (
                <li key={e.id} className={cn('overflow-hidden rounded-lg border bg-card', on && 'border-primary ring-2 ring-primary/40')}>
                  <div className="group relative aspect-square bg-muted/50">
                    {e.kind ? <Thumb src={mediaUrl(e.id, 'thumb', e.hash)} className="size-full opacity-70" fallback={<File className="size-8 text-muted-foreground" />} /> : <div className="grid size-full place-items-center"><File className="size-8 text-muted-foreground" /></div>}
                    <Checkbox checked={on} onCheckedChange={() => toggle(e.id)} aria-label={t('Select {name}', { name: e.name })} className={cn('absolute top-2 left-2 bg-background/90', !on && 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100 max-sm:opacity-100')} />
                  </div>
                  <div className="space-y-1 p-2.5">
                    <p className="truncate text-sm font-medium" title={`${e.folder}/${e.path}`}>{e.name}</p>
                    <p className="truncate text-xs text-muted-foreground">{e.folder} · {bytes(e.size)}</p>
                    <p className="text-xs text-muted-foreground" title={t('Removed for good on {date}', { date: date(e.purge_at) })}>{t('Deleted {when} · gone {date}', { when: ago(e.deleted_at, now), date: date(e.purge_at) })}</p>
                    <div className="mt-1 flex gap-1.5">
                      <Button size="sm" variant="outline" className="flex-1" disabled={restore.isPending || purge.isPending} onClick={() => restore.mutate([e])}><RotateCcw className="size-3.5" />{t('Restore')}</Button>
                      <Button size="sm" variant="outline" className="text-destructive hover:text-destructive" disabled={restore.isPending || purge.isPending} onClick={() => setPurging([e])} aria-label={t('Delete forever')} title={t('Delete forever')}><Trash2 className="size-3.5" /></Button>
                    </div>
                  </div>
                </li>
              )
            })}
          </ul>
          <div className="mt-3 flex items-center justify-between text-sm text-muted-foreground">
            <span>{t('{shown} of {total}', { shown: count(entries.length), total: count(total) })}</span>
            {trash.hasNextPage && <Button variant="outline" size="sm" disabled={trash.isFetchingNextPage} onClick={() => void trash.fetchNextPage()}>{trash.isFetchingNextPage ? t('Loading…') : t('Load more')}</Button>}
          </div>
        </>
      )}
      <Confirm
        open={purging !== null}
        destructive
        title={t('Delete forever?')}
        description={t('{files} will be removed for good, with their versions, albums and tags. This cannot be undone.', { files: plural(purging?.length ?? 0, 'file') })}
        action={t('Delete forever')}
        onConfirm={() => purging && purge.mutate(purging)}
        onClose={() => setPurging(null)}
      />
    </div>
  )
}
