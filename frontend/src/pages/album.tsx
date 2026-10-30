import { useMutation, useQuery } from '@tanstack/react-query'
import { ArrowLeft, FolderOpen, Image as ImageIcon, Pencil, Trash2 } from 'lucide-react'
import { useCallback, useMemo, useState, type FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { toast } from 'sonner'
import { Confirm } from '@/components/confirm'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { DayGrid } from '@/components/library/day-grid'
import { FolderGrid } from '@/components/library/folder-grid'
import { SelectionBar } from '@/components/library/selection-bar'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import { api, mediaUrl, query } from '@/lib/api'
import { bytes, plural } from '@/lib/format'
import { useRefreshLibrary } from '@/lib/library'
import { useOpenMedia, useViewerSource } from '@/lib/viewer-source'
import type { AlbumInfo, Asset } from '@/lib/types'
import { useT } from '@/lib/i18n'

/** Cells shown at first; an album of thousands would otherwise draw them all at once. */
const FIRST = 2000

function EditAlbum({ album, onClose }: { album: AlbumInfo; onClose: () => void }) {
  const t = useT()
  const refresh = useRefreshLibrary()
  const [name, setName] = useState(album.name)
  const [description, setDescription] = useState(album.description)
  const save = useMutation({
    mutationFn: () => api.patch(`/api/albums/${album.id}`, { name, description }),
    onSuccess: () => {
      void refresh()
      onClose()
    },
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (name.trim()) save.mutate()
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Edit album')}</DialogTitle>
            <DialogDescription>{t('The name and description show on the album.')}</DialogDescription>
          </DialogHeader>
          <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} aria-label={t('Album name')} maxLength={120} />
          <Textarea value={description} onChange={(e) => setDescription(e.target.value)} placeholder={t('Description (optional)')} aria-label={t('Description')} rows={3} />
          <DialogFooter><Button type="submit" disabled={!name.trim() || save.isPending}>{t('Save')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Album() {
  const t = useT()
  const id = useParams().id ?? ""
  const navigate = useNavigate()
  const refresh = useRefreshLibrary()
  const { data, error } = useQuery({ queryKey: ['album', id], queryFn: () => api.get<{ album: AlbumInfo; assets: Asset[] }>(`/api/albums/${id}`), enabled: id !== "" })
  const [selected, setSelected] = useState<Map<string, Asset>>(new Map())
  const openMedia = useOpenMedia()
  const [dialog, setDialog] = useState<'edit' | 'delete' | null>(null)
  const [showAll, setShowAll] = useState(false)

  const assets = data?.assets
  const shown = useMemo(() => (showAll ? assets : assets?.slice(0, FIRST)) ?? [], [assets, showAll])
  const byId = useMemo(() => new Map((assets ?? []).map((a) => [a.id, a])), [assets])
  const ids = useMemo(() => new Set(selected.keys()), [selected])
  const toggle = useCallback((id: string) => setSelected((s) => { const n = new Map(s); const a = byId.get(id); if (!n.delete(id) && a) n.set(id, a); return n }), [byId])
  const toggleMany = useCallback((list: string[], on: boolean) => setSelected((s) => { const n = new Map(s); list.forEach((id) => { const a = byId.get(id); if (on && a) n.set(id, a); else n.delete(id) }); return n }), [byId])
  const clear = useCallback(() => setSelected(new Map()), [])
  const source = useMemo(() => (assets ? { assets } : null), [assets])
  useViewerSource(source)

  const cover = useMutation({
    mutationFn: (file: string) => api.patch(`/api/albums/${id}`, { cover: file }),
    onSuccess: () => { toast.success(t('Cover changed')); clear(); void refresh() },
    onError: (e) => toast.error(e.message),
  })
  const remove = useMutation({
    mutationFn: () => api.delete(`/api/albums/${id}`),
    onSuccess: () => { toast.success(t('Album deleted. Its photos are untouched.')); void refresh(); navigate('/albums') },
    onError: (e) => toast.error(e.message),
  })

  if (error) {
    return (
      <div className="space-y-3">
        <ErrorNote error={error} />
        <p className="text-sm text-muted-foreground">{t('This link is not valid any more, or the file is gone.')}</p>
        <Button variant="outline" asChild><Link to="/albums">{t('Back to Albums')}</Link></Button>
      </div>
    )
  }
  if (!data) return <Skeleton className="h-96" />
  const { album } = data
  const folder = album.folder

  return (
    <div>
      <Link to="/albums" className="mb-3 inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"><ArrowLeft className="size-4" />{t('Albums')}</Link>
      <PageHeader
        title={album.name}
        description={
          <>
            {plural(album.count, 'item')} · {bytes(album.bytes)}
            {folder && <span className="mt-1 block">{t('Shows the folder {folder}', { folder: [folder.device, folder.path].filter(Boolean).join('/') })}</span>}
            {album.description && <span className="mt-1 block whitespace-pre-line">{album.description}</span>}
          </>
        }
        actions={
          <>
            {folder && <Button variant="outline" asChild><Link to={`/files${query({ device: folder.device, path: folder.path })}`}><FolderOpen className="size-4" />{t('Open folder')}</Link></Button>}
            <Button variant="outline" onClick={() => setDialog('edit')}><Pencil className="size-4" />{t('Edit')}</Button>
            <Button variant="outline" onClick={() => setDialog('delete')}><Trash2 className="size-4" />{t('Delete album')}</Button>
          </>
        }
      />

      {selected.size > 0 && (
        <div className="sticky top-14 z-20 -mx-3 mb-2 flex items-center gap-2 bg-background/90 px-3 py-2 backdrop-blur md:-mx-6 md:px-6">
          <div className="min-w-0 flex-1"><SelectionBar selected={[...selected.values()]} onClear={clear} albumId={folder ? undefined : id} /></div>
          {selected.size === 1 && <Button variant="outline" size="sm" onClick={() => cover.mutate([...selected.keys()][0])}><ImageIcon className="size-4" />{t('Use as cover')}</Button>}
        </div>
      )}

      {assets && assets.length === 0 ? (
        <Empty title={t('This album is empty')}>{folder ? t('No photos or videos in this folder yet.') : t('Open the timeline, select photos and choose Add to album.')}</Empty>
      ) : (
        <>
          {folder ? (
            <FolderGrid assets={shown} root={folder.path} src={(a) => mediaUrl(a.id, 'thumb', a.hash)} selected={ids} onOpen={(a) => openMedia(a.id)} onToggle={toggle} />
          ) : (
            <DayGrid assets={shown} src={(a) => mediaUrl(a.id, 'thumb', a.hash)} selected={ids} onOpen={(a) => openMedia(a.id)} onToggle={toggle} onToggleMany={toggleMany} />
          )}
          {assets && assets.length > shown.length && (
            <div className="py-4 text-center"><Button variant="outline" onClick={() => setShowAll(true)}>{t('Show all {count}', { count: assets.length.toLocaleString() })}</Button></div>
          )}
        </>
      )}

      {dialog === 'edit' && <EditAlbum album={album} onClose={() => setDialog(null)} />}
      <Confirm
        open={dialog === 'delete'}
        title={t('Delete the album “{name}”?', { name: album.name })}
        description={t('The photos and videos stay where they are.')}
        action={t('Delete album')}
        destructive
        onCancel={() => setDialog(null)}
        onConfirm={() => { setDialog(null); remove.mutate() }}
      />
    </div>
  )
}
