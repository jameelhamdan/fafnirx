import { useQuery } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { Images } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { FormDialog } from '../form-dialog'
import { Button } from '../ui/button'
import { Input } from '../ui/input'
import { Label } from '../ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import type { AlbumInfo } from '@/lib/types'

export interface FolderRef {
  folder: string
  path: string
}

const same = (a: string, b: string) => a.replace(/^\/+|\/+$/g, '') === b.replace(/^\/+|\/+$/g, '')

/** The album a folder is, if it is one: an album is a folder shown as a collection, with a name of its own. */
function useFolderAlbum(folder: FolderRef | null): AlbumInfo | undefined {
  const { data } = useQuery({ queryKey: ['albums'], queryFn: () => api.get<AlbumInfo[]>('/api/albums'), enabled: folder !== null })
  return folder ? data?.find((a) => a.folder.folder === folder.folder && same(a.folder.path, folder.path)) : undefined
}

/**
 * Makes a folder an album, renames the album, or ends it (the files stay). An album is the folder seen as a collection, a show or a
 * trip: it holds whatever is in the folder, so there is nothing to keep up.
 */
const ALBUM_KEYS = [['albums'], ['album']]

export function FolderAlbum({ folder, onClose }: { folder: FolderRef | null; onClose: () => void }) {
  const t = useT()
  const album = useFolderAlbum(folder)
  const [typed, setTyped] = useState<string | null>(null)
  const fallback = (folder?.path || folder?.folder || '').split('/').pop() ?? ''
  const name = typed ?? album?.name ?? fallback
  const done = () => {
    setTyped(null)
    onClose()
  }
  const save = useApiMutation(() => (album ? api.patch(`/api/albums/${album.id}`, { name }) : api.post('/api/albums', { name: name.trim(), folder })), {
    invalidate: ALBUM_KEYS,
    onSuccess: () => { toast.success(album ? t('Album saved.') : t('The folder is an album now.')); done() },
  })
  const end = useApiMutation(() => api.delete(`/api/albums/${album?.id ?? ''}`), {
    invalidate: ALBUM_KEYS,
    onSuccess: () => { toast.success(t('The folder is not an album any more. Its files are as they were.')); done() },
  })
  return (
    <FormDialog
      open={folder !== null}
      onOpenChange={(o) => { if (!o) { setTyped(null); onClose() } }}
      title={album ? t('Album of this folder') : t('Show this folder as an album')}
      description={t('An album shows everything in the folder, new files included, and follows the folder if it is renamed. It is yours alone.')}
      onSubmit={() => save.mutate()}
      submit={album ? t('Save') : t('Make album')}
      disabled={!name.trim()}
      pending={save.isPending}
      footer={
        album && (
          <span className="flex gap-2">
            <Button type="button" variant="outline" asChild><Link to={`/albums/${album.id}`}><Images className="size-4" />{t('Open album')}</Link></Button>
            <Button type="button" variant="outline" disabled={end.isPending} onClick={() => end.mutate()}>{t('Stop being an album')}</Button>
          </span>
        )
      }
    >
      <div className="space-y-2">
        <Label htmlFor="album-name">{t('Name')}</Label>
        <Input id="album-name" autoFocus value={name} onChange={(e) => setTyped(e.target.value)} />
      </div>
    </FormDialog>
  )
}

/** The button for the folder being looked at: it says whether the folder is an album, and opens the dialog above. */
export function FolderAlbumButton({ folder }: { folder: FolderRef }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const album = useFolderAlbum(folder)
  return (
    <>
      <Button variant={album ? 'secondary' : 'outline'} size="sm" onClick={() => setOpen(true)}>
        <Images className="size-4" />{album ? t('Album: {name}', { name: album.name }) : t('Show as album')}
      </Button>
      <FolderAlbum folder={open ? folder : null} onClose={() => setOpen(false)} />
    </>
  )
}
