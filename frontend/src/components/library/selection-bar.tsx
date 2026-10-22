import { Archive, ArchiveRestore, Download, FolderMinus, FolderPlus, Heart, MoreHorizontal, Tag, Trash2, X } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { Button } from '../ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../ui/dropdown-menu'
import { Confirm } from '../confirm'
import { api, downloadZip } from '@/lib/api'
import { plural } from '@/lib/format'
import { useLibraryActions, useRefreshLibrary } from '@/lib/library'
import type { Asset } from '@/lib/types'
import { AlbumPicker } from './album-picker'
import { TagDialog } from './tag-dialog'

/** What can be done to the selected photos and videos. Sticks to the top while you scroll. */
export function SelectionBar({ selected, onClear, albumId }: { selected: Asset[]; onClear: () => void; albumId?: number }) {
  const { bulk, remove } = useLibraryActions()
  const refresh = useRefreshLibrary()
  const [dialog, setDialog] = useState<'album' | 'tags' | 'delete' | null>(null)
  if (selected.length === 0) return null
  const ids = selected.map((a) => a.id)
  const allFavorite = selected.every((a) => a.favorite)
  const allArchived = selected.every((a) => a.archived)
  const finish = () => {
    setDialog(null)
    onClear()
  }

  const leaveAlbum = async () => {
    try {
      await api.post(`/api/albums/${albumId}/files/remove`, { ids })
      toast.success(`Removed ${plural(ids.length, 'file')} from the album`)
      onClear()
      void refresh()
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Could not remove them')
    }
  }

  return (
    <div role="toolbar" aria-label="Selection" className="flex items-center gap-1 rounded-lg border bg-popover p-1.5 shadow-md">
      <Button variant="ghost" size="icon" className="size-8" aria-label="Clear selection" onClick={onClear}><X className="size-4" /></Button>
      <span className="mr-auto px-1 text-sm font-medium">{selected.length} selected</span>
      <Button variant="ghost" size="sm" aria-label={allFavorite ? 'Remove from favorites' : 'Add to favorites'} onClick={() => bulk.mutate({ ids, favorite: !allFavorite }, { onSuccess: onClear })}>
        <Heart className={allFavorite ? 'size-4 fill-current' : 'size-4'} /><span className="max-sm:sr-only">{allFavorite ? 'Unfavorite' : 'Favorite'}</span>
      </Button>
      <Button variant="ghost" size="sm" onClick={() => setDialog('album')}><FolderPlus className="size-4" /><span className="max-sm:sr-only">Add to album</span></Button>
      {albumId !== undefined && <Button variant="ghost" size="sm" onClick={() => void leaveAlbum()}><FolderMinus className="size-4" /><span className="max-sm:sr-only">Remove from album</span></Button>}
      <DropdownMenu>
        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="size-8" aria-label="More actions"><MoreHorizontal className="size-4" /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={() => setDialog('tags')}><Tag className="size-4" />Add tags</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => bulk.mutate({ ids, archived: !allArchived }, { onSuccess: onClear })}>
            {allArchived ? <ArchiveRestore className="size-4" /> : <Archive className="size-4" />}{allArchived ? 'Unarchive' : 'Archive'}
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => downloadZip(selected.map((a) => ({ device: a.device, path: a.path })), selected.length === 1 ? selected[0].name : 'photos')}>
            <Download className="size-4" />Download
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem variant="destructive" onSelect={() => setDialog('delete')}><Trash2 className="size-4" />Delete</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      <AlbumPicker open={dialog === 'album'} ids={ids} onClose={() => setDialog(null)} onDone={finish} />
      <TagDialog open={dialog === 'tags'} ids={ids} onClose={() => setDialog(null)} onDone={finish} />
      <Confirm
        open={dialog === 'delete'}
        title={`Delete ${plural(selected.length, 'file')}?`}
        description="They move to the trash and stay recoverable until the retention period ends. The originals on your devices are not touched."
        action="Delete"
        destructive
        onCancel={() => setDialog(null)}
        onConfirm={() => remove.mutate(selected.map((a) => ({ device: a.device, path: a.path })), { onSuccess: finish })}
      />
    </div>
  )
}
