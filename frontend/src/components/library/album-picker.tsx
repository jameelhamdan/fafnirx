import { useMutation, useQueryClient } from '@tanstack/react-query'
import { FolderPlus, Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { Skeleton } from '../ui/skeleton'
import { api, mediaUrl } from '@/lib/api'
import { plural } from '@/lib/format'
import { useAlbums, useRefreshLibrary } from '@/lib/library'
import type { AlbumInfo } from '@/lib/types'
import { Thumb } from './thumb'
import { useT } from '@/lib/i18n'

/** Adds files to an album you pick, or to a new one. */
export function AlbumPicker({ open, ids, onClose, onDone }: { open: boolean; ids: number[]; onClose: () => void; onDone?: () => void }) {
  const t = useT()
  const client = useQueryClient()
  const refresh = useRefreshLibrary()
  const albums = useAlbums(open)
  const [name, setName] = useState('')

  const done = (album: AlbumInfo, added: number) => {
    toast.success(added === ids.length ? t('Added {files} to {album}', { files: plural(added, 'file'), album: album.name }) : t('Added {added} of {total} to {album} (the rest were already there)', { added, total: ids.length, album: album.name }))
    void refresh()
    void client.invalidateQueries({ queryKey: ['albums'] })
    setName('')
    onClose()
    onDone?.()
  }
  const add = useMutation({
    mutationFn: async (album: AlbumInfo) => ({ album, added: (await api.post<{ count: number }>(`/api/albums/${album.id}/files`, { ids })).count }),
    onSuccess: ({ album, added }) => done(album, added),
    onError: (e) => toast.error(e.message),
  })
  const create = useMutation({
    mutationFn: () => api.post<AlbumInfo>('/api/albums', { name, ids }),
    onSuccess: (album) => done(album, ids.length),
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim()) create.mutate()
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('Add to album')}</DialogTitle>
          <DialogDescription>{t('{items} selected.', { items: plural(ids.length, 'photo or video') })}</DialogDescription>
        </DialogHeader>
        <form onSubmit={submit} className="flex gap-2">
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder={t('New album name')} aria-label={t('New album name')} maxLength={120} />
          <Button type="submit" disabled={!name.trim() || create.isPending}><Plus className="size-4" />{t('Create')}</Button>
        </form>
        <ul className="-mx-2 max-h-72 space-y-1 overflow-y-auto px-2">
          {albums.isPending && <Skeleton className="h-12" />}
          {albums.data?.length === 0 && <li className="py-3 text-center text-sm text-muted-foreground">{t('No albums yet. Name one above.')}</li>}
          {albums.data?.map((a) => (
            <li key={a.id}>
              <button type="button" disabled={add.isPending} onClick={() => add.mutate(a)} className="flex w-full items-center gap-3 rounded-md p-2 text-left hover:bg-accent disabled:opacity-50">
                <span className="grid size-11 shrink-0 place-items-center overflow-hidden rounded-md bg-muted">
                  {a.cover ? <Thumb src={mediaUrl(a.cover, 'thumb')} className="size-full" fallback={<FolderPlus className="size-4 text-muted-foreground" />} /> : <FolderPlus className="size-4 text-muted-foreground" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{a.name}</span>
                  <span className="block text-xs text-muted-foreground">{plural(a.count, 'item')}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  )
}
