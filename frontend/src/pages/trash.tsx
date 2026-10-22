import { useInfiniteQuery, useMutation } from '@tanstack/react-query'
import { File, RotateCcw } from 'lucide-react'
import { useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
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

/** Deleted files of every device. They are removed for good when the retention period ends; until then they come back with one click. */
export default function TrashPage() {
  const now = useNow()
  const refresh = useRefreshLibrary()
  const [selected, setSelected] = useState<Set<number>>(new Set())
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

  const restore = useMutation({
    mutationFn: async (list: TrashEntry[]) => {
      let done = 0
      for (const e of list) {
        await api.post('/api/files/restore', { device: e.device, path: e.path, version: e.version })
        done++
      }
      return done
    },
    onSuccess: (done) => {
      toast.success(`Restored ${plural(done, 'file')}`)
      setSelected(new Set())
      void refresh()
    },
    onError: (e) => {
      toast.error(e.message)
      void refresh()
    },
  })
  const toggle = (id: number) => setSelected((s) => { const n = new Set(s); if (!n.delete(id)) n.add(id); return n })

  return (
    <div>
      <PageHeader title="Trash" description={`Deleted files stay here for ${retention} days, then they are removed for good. Restoring puts a file back where it was, with its albums, tags and favorite.`} />
      {chosen.length > 0 && (
        <div className="mb-3 flex items-center gap-2 rounded-lg border bg-popover p-2 shadow-sm">
          <span className="px-1 text-sm font-medium">{chosen.length} selected</span>
          <Button size="sm" className="ml-auto" disabled={restore.isPending} onClick={() => restore.mutate(chosen)}><RotateCcw className="size-4" />Restore</Button>
          <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>Clear</Button>
        </div>
      )}
      {trash.error ? <ErrorNote error={trash.error} /> : !trash.data ? (
        <Skeleton className="h-64" />
      ) : entries.length === 0 ? (
        <Empty title="The trash is empty">Files you delete from the web app, or that disappear from a device, wait here before they are removed for good.</Empty>
      ) : (
        <>
          <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
            {entries.map((e) => {
              const on = selected.has(e.id)
              return (
                <li key={e.id} className={cn('overflow-hidden rounded-lg border bg-card', on && 'border-primary ring-2 ring-primary/40')}>
                  <div className="group relative aspect-square bg-muted/50">
                    {e.kind ? <Thumb src={mediaUrl(e.id, 'thumb', e.hash)} className="size-full opacity-70" fallback={<File className="size-8 text-muted-foreground" />} /> : <div className="grid size-full place-items-center"><File className="size-8 text-muted-foreground" /></div>}
                    <Checkbox checked={on} onCheckedChange={() => toggle(e.id)} aria-label={`Select ${e.name}`} className={cn('absolute top-2 left-2 bg-background/90', !on && 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100 max-sm:opacity-100')} />
                  </div>
                  <div className="space-y-1 p-2.5">
                    <p className="truncate text-sm font-medium" title={`${e.device}/${e.path}`}>{e.name}</p>
                    <p className="truncate text-xs text-muted-foreground">{e.device} · {bytes(e.size)}</p>
                    <p className="text-xs text-muted-foreground" title={`Removed for good on ${date(e.purge_at)}`}>Deleted {ago(e.deleted_at, now)} · gone {date(e.purge_at)}</p>
                    <Button size="sm" variant="outline" className="mt-1 w-full" disabled={restore.isPending} onClick={() => restore.mutate([e])}><RotateCcw className="size-3.5" />Restore</Button>
                  </div>
                </li>
              )
            })}
          </ul>
          <div className="mt-3 flex items-center justify-between text-sm text-muted-foreground">
            <span>{count(entries.length)} of {count(total)}</span>
            {trash.hasNextPage && <Button variant="outline" size="sm" disabled={trash.isFetchingNextPage} onClick={() => void trash.fetchNextPage()}>{trash.isFetchingNextPage ? 'Loading…' : 'Load more'}</Button>}
          </div>
        </>
      )}
    </div>
  )
}
