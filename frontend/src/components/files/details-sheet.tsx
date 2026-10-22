import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, ExternalLink, History, RotateCcw, Trash2 } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { toast } from 'sonner'
import { ErrorNote } from '../common'
import { Confirm } from '../confirm'
import { Badge } from '../ui/badge'
import { Button } from '../ui/button'
import { Separator } from '../ui/separator'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '../ui/sheet'
import { Skeleton } from '../ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api, downloadUrl, thumbUrl } from '@/lib/api'
import { ago, bytes, dateTime } from '@/lib/format'
import type { FileInfo } from '@/lib/types'
import { FileIcon } from './file-icon'
import { useT } from '@/lib/i18n'

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 break-words text-sm">{children}</dd>
    </div>
  )
}

function Preview({ info }: { info: FileInfo }) {
  const url = downloadUrl(info.device, info.path, { inline: true })
  // A video has a poster picture too, but what people want from it here is to play it.
  if (info.mime.startsWith('video/')) {
    return <video src={url} poster={info.preview ? thumbUrl(info.device, info.path, 1000) : undefined} controls preload="none" className="max-h-80 w-full rounded-lg border bg-black" />
  }
  if (info.preview) {
    return (
      <a href={url} target="_blank" rel="noreferrer" className="block overflow-hidden rounded-lg border bg-muted/40">
        <img src={thumbUrl(info.device, info.path, 1000)} alt={info.name} className="mx-auto max-h-80 w-full object-contain" />
      </a>
    )
  }
  if (info.mime.startsWith('audio/')) return <audio src={url} controls className="w-full" />
  return (
    <div className="grid h-32 place-items-center rounded-lg border bg-muted/40">
      <FileIcon entry={{ kind: 'file', mime: info.mime }} className="size-10" />
    </div>
  )
}

/** Everything about one file: a preview, its metadata, and its history with download and restore. */
export function DetailsSheet({ device, path, onClose, onDeleted }: { device: string; path: string | null; onClose: () => void; onDeleted: () => void }) {
  const t = useT()
  const client = useQueryClient()
  const now = useNow()
  const [confirm, setConfirm] = useState<'delete' | number | null>(null)
  const { data: info, error } = useQuery({
    queryKey: ['info', device, path],
    queryFn: () => api.get<FileInfo>(`/api/files/info?device=${encodeURIComponent(device)}&path=${encodeURIComponent(path ?? '')}`),
    enabled: path !== null,
  })
  const refresh = () => Promise.all([client.invalidateQueries({ queryKey: ['info'] }), client.invalidateQueries({ queryKey: ['files'] })])

  const remove = useMutation({
    mutationFn: () => api.post('/api/files/delete', { items: [{ device, path }] }),
    onSuccess: () => { toast.success(t('Deleted. It stays recoverable until the retention period ends.')); onDeleted() },
    onError: (e) => toast.error(e.message),
  })
  const restore = useMutation({
    mutationFn: (version: number) => api.post('/api/files/restore', { device, path, version }),
    onSuccess: () => { toast.success(t('Restored as the newest version')); void refresh() },
    onError: (e) => toast.error(e.message),
  })

  const exif = info?.exif ? Object.entries(info.exif) : []
  return (
    <Sheet open={path !== null} onOpenChange={(o) => !o && onClose()}>
      <SheetContent className="w-full gap-0 overflow-y-auto sm:max-w-lg">
        <SheetHeader>
          <SheetTitle className="truncate pr-8">{info?.name ?? path?.split('/').pop()}</SheetTitle>
          <SheetDescription className="truncate">{device}/{path}</SheetDescription>
        </SheetHeader>
        {error ? <div className="p-4"><ErrorNote error={error} /></div> : !info ? (
          <div className="space-y-3 p-4"><Skeleton className="h-48" /><Skeleton className="h-24" /></div>
        ) : (
          <div className="space-y-5 p-4 pt-0">
            <Preview info={info} />
            <div className="flex flex-wrap gap-2">
              <Button asChild><a href={downloadUrl(device, info.path)}><Download className="size-4" />{t('Download')}</a></Button>
              {(info.preview || info.mime.startsWith('video/') || info.mime === 'application/pdf') && (
                <Button variant="outline" asChild><a href={downloadUrl(device, info.path, { inline: true })} target="_blank" rel="noreferrer"><ExternalLink className="size-4" />{t('Open')}</a></Button>
              )}
              {!info.deleted_at && <Button variant="outline" onClick={() => setConfirm('delete')}><Trash2 className="size-4" />{t('Delete')}</Button>}
            </div>
            {info.deleted_at && (
              <p className="rounded-md border border-warning/40 bg-warning/10 p-3 text-sm">
                {t('Deleted {when}. It is removed for good on {date}. Restore a version below to bring it back.', { when: ago(info.deleted_at, now), date: dateTime(info.purge_at) })}
              </p>
            )}

            <dl className="grid grid-cols-2 gap-4">
              <Fact label={t('Size')}>{bytes(info.size)}</Fact>
              <Fact label={t('Type')}>{info.mime}</Fact>
              {info.width && info.height && <Fact label={t('Dimensions')}>{info.width} × {info.height}</Fact>}
              {info.taken_at && <Fact label={t('Taken')}>{dateTime(info.taken_at)}</Fact>}
              <Fact label={t('Modified on the device')}>{dateTime(info.mtime)}</Fact>
              <Fact label={t('Recorded here')}>{dateTime(info.changed)}</Fact>
              <div className="col-span-2"><Fact label={t('Content hash (BLAKE3 root)')}><code className="text-xs break-all">{info.hash}</code></Fact></div>
            </dl>

            {exif.length > 0 && (
              <>
                <Separator />
                <section>
                  <h3 className="mb-2 text-sm font-medium">{t('Photo details')}</h3>
                  <dl className="grid grid-cols-2 gap-x-4 gap-y-2">{exif.map(([k, v]) => <Fact key={k} label={k.replace(/([a-z])([A-Z])/g, '$1 $2')}>{v}</Fact>)}</dl>
                </section>
              </>
            )}

            <Separator />
            <section>
              <h3 className="mb-2 flex items-center gap-2 text-sm font-medium"><History className="size-4" />{t('History')}</h3>
              <ul className="divide-y rounded-lg border">
                {info.versions.map((v) => (
                  <li key={v.id} className="flex flex-wrap items-center gap-2 px-3 py-2 text-sm">
                    <div className="min-w-0 flex-1">
                      <p className="flex items-center gap-2">{dateTime(v.created_at)}{v.current && !info.deleted_at && <Badge>{t('Current')}</Badge>}{v.pruned_at && <Badge variant="secondary">{t('Expiring')}</Badge>}</p>
                      <p className="text-xs text-muted-foreground">{bytes(v.size)}</p>
                    </div>
                    {v.available ? (
                      <>
                        <Button variant="ghost" size="icon" className="size-8" asChild aria-label={t('Download this version')}><a href={downloadUrl(device, info.path, { version: v.id })}><Download className="size-4" /></a></Button>
                        {(!v.current || info.deleted_at) && (
                          <Button variant="ghost" size="icon" className="size-8" aria-label={t('Restore this version')} onClick={() => setConfirm(v.id)}><RotateCcw className="size-4" /></Button>
                        )}
                      </>
                    ) : <span className="text-xs text-muted-foreground">{t('No longer stored')}</span>}
                  </li>
                ))}
              </ul>
            </section>

            {info.events.length > 0 && (
              <section>
                <h3 className="mb-2 text-sm font-medium">{t('Activity')}</h3>
                <ul className="space-y-1 text-sm">
                  {info.events.map((e) => (
                    <li key={e.id} className="flex justify-between gap-3">
                      <span><span className="font-medium capitalize">{e.action}</span>{e.detail && <span className="text-muted-foreground"> · {e.detail}</span>}</span>
                      <span className="shrink-0 text-xs text-muted-foreground">{ago(e.ts, now)}</span>
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </div>
        )}
        <Confirm
          open={confirm === 'delete'}
          title={t('Delete this file?')}
          description={t('It leaves the file list but stays recoverable until the retention period ends.')}
          action={t('Delete')}
          destructive
          onCancel={() => setConfirm(null)}
          onConfirm={() => { setConfirm(null); remove.mutate() }}
        />
        <Confirm
          open={typeof confirm === 'number'}
          title={t('Restore this version?')}
          description={t('It becomes the newest version. The current content stays in the history.')}
          action={t('Restore')}
          onCancel={() => setConfirm(null)}
          onConfirm={() => { if (typeof confirm === 'number') restore.mutate(confirm); setConfirm(null) }}
        />
      </SheetContent>
    </Sheet>
  )
}
