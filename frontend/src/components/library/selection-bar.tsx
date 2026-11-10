import { Archive, ArchiveRestore, Download, Heart, MoreHorizontal, Tag, Trash2, X } from 'lucide-react'
import { useState } from 'react'
import { Button } from '../ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../ui/dropdown-menu'
import { Confirm } from '../confirm'
import { downloadZip } from '@/lib/api'
import { plural } from '@/lib/format'
import { useLibraryActions } from '@/lib/library'
import { hasWorkspace, useRole } from '@/lib/session'
import type { Asset } from '@/lib/types'
import { TagDialog } from './tag-dialog'
import { useT } from '@/lib/i18n'

/** What can be done to the selected photos and videos. Sticks to the top while you scroll. */
export function SelectionBar({ selected, onClear }: { selected: Asset[]; onClear: () => void }) {
  const mine = hasWorkspace(useRole())
  const { bulk, remove } = useLibraryActions()
  const t = useT()
  const [dialog, setDialog] = useState<'tags' | 'delete' | null>(null)
  if (selected.length === 0) return null
  const ids = selected.map((a) => a.id)
  const allFavorite = selected.every((a) => a.favorite)
  const allArchived = selected.every((a) => a.archived)
  const finish = () => {
    setDialog(null)
    onClear()
  }

  return (
    <div role="toolbar" aria-label={t('Selection')} className="flex items-center gap-1 rounded-md border bg-popover p-1.5 shadow-md">
      <Button variant="ghost" size="icon" className="size-8" aria-label={t('Clear selection')} onClick={onClear}><X className="size-4" /></Button>
      <span className="mr-auto px-1 text-sm font-medium">{t('{count} selected', { count: selected.length })}</span>
      {mine && (
        <Button variant="ghost" size="sm" aria-label={allFavorite ? t('Remove from favorites') : t('Add to favorites')} onClick={() => bulk.mutate({ ids, favorite: !allFavorite }, { onSuccess: onClear })}>
          <Heart className={allFavorite ? 'size-4 fill-current' : 'size-4'} /><span className="max-sm:sr-only">{allFavorite ? t('Unfavorite') : t('Favorite')}</span>
        </Button>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="size-8" aria-label={t('More actions')}><MoreHorizontal className="size-4" /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {mine && <DropdownMenuItem onSelect={() => setDialog('tags')}><Tag className="size-4" />{t('Add tags')}</DropdownMenuItem>}
          {mine && (
            <DropdownMenuItem onSelect={() => bulk.mutate({ ids, archived: !allArchived }, { onSuccess: onClear })}>
              {allArchived ? <ArchiveRestore className="size-4" /> : <Archive className="size-4" />}{allArchived ? t('Unarchive') : t('Archive')}
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onSelect={() => downloadZip(selected.map((a) => ({ device: a.device, path: a.path })), selected.length === 1 ? selected[0].name : 'photos')}>
            <Download className="size-4" />{t('Download')}
          </DropdownMenuItem>
          {mine && <DropdownMenuSeparator />}
          {mine && <DropdownMenuItem variant="destructive" onSelect={() => setDialog('delete')}><Trash2 className="size-4" />{t('Delete')}</DropdownMenuItem>}
        </DropdownMenuContent>
      </DropdownMenu>

      <TagDialog open={dialog === 'tags'} ids={ids} onClose={() => setDialog(null)} onDone={finish} />
      <Confirm
        open={dialog === 'delete'}
        title={t('Delete {files}?', { files: plural(selected.length, 'file') })}
        description={t('They move to the trash and stay recoverable until the retention period ends. The originals on your devices are not touched.')}
        action={t('Delete')}
        destructive
        onCancel={() => setDialog(null)}
        onConfirm={() => remove.mutate(selected.map((a) => ({ device: a.device, path: a.path })), { onSuccess: finish })}
      />
    </div>
  )
}
