import { useMutation } from '@tanstack/react-query'
import { Images, Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Thumb } from '@/components/library/thumb'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { api, mediaUrl } from '@/lib/api'
import { bytes, plural } from '@/lib/format'
import { useAlbums, useRefreshLibrary } from '@/lib/library'
import type { AlbumInfo } from '@/lib/types'
import { useT } from '@/lib/i18n'

function NewAlbum({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT()
  const refresh = useRefreshLibrary()
  const navigate = useNavigate()
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post<AlbumInfo>('/api/albums', { name }),
    onSuccess: (album) => {
      void refresh()
      setName('')
      onClose()
      navigate(`/albums/${album.id}`)
    },
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim()) create.mutate()
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('New album')}</DialogTitle>
            <DialogDescription>{t('An album groups photos and videos without moving them. Add to it from the timeline: select some and choose Add to album.')}</DialogDescription>
          </DialogHeader>
          <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t('Summer 2024')} aria-label={t('Album name')} maxLength={120} />
          <DialogFooter><Button type="submit" disabled={!name.trim() || create.isPending}>{t('Create')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Albums() {
  const t = useT()
  const albums = useAlbums()
  const [creating, setCreating] = useState(false)
  return (
    <div>
      <PageHeader
        title={t('Albums')}
        description={t('Groups of photos and videos. The files stay where they are; deleting an album never deletes them.')}
        actions={<Button onClick={() => setCreating(true)}><Plus className="size-4" />{t('New album')}</Button>}
      />
      {albums.error ? <ErrorNote error={albums.error} /> : !albums.data ? (
        <Skeleton className="h-64" />
      ) : albums.data.length === 0 ? (
        <Empty title={t('No albums yet')}>{t('Create one, then add photos to it from the timeline.')}</Empty>
      ) : (
        <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {albums.data.map((a) => (
            <li key={a.id}>
              <Link to={`/albums/${a.id}`} className="group block overflow-hidden rounded-xl border bg-card transition-shadow hover:shadow-md focus-visible:ring-2 focus-visible:ring-ring">
                <div className="relative aspect-square bg-muted">
                  {a.cover ? (
                    <Thumb src={mediaUrl(a.cover, 'thumb')} className="size-full transition-transform group-hover:scale-105" fallback={<Images className="size-8 text-muted-foreground" />} />
                  ) : (
                    <div className="grid size-full place-items-center"><Images className="size-8 text-muted-foreground" /></div>
                  )}
                </div>
                <div className="p-3">
                  <p className="truncate font-medium">{a.name}</p>
                  <p className="text-xs text-muted-foreground">{plural(a.count, 'item')} · {bytes(a.bytes)}</p>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
      <NewAlbum open={creating} onClose={() => setCreating(false)} />
    </div>
  )
}
