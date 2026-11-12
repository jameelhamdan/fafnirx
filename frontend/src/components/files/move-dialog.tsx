import { useApiMutation } from '@/lib/mutation'
import { useState } from 'react'
import { toast } from 'sonner'
import { FormDialog } from '../form-dialog'
import { Input } from '../ui/input'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import { useRefreshLibrary } from '@/lib/library'

/**
 * Renames or moves a file, or a folder with what is in it, within a folder the master owns (a local or shared folder, or the
 * master files). The new place is a full path on the same folder; folders in it are made as needed.
 */
export function MoveDialog({ folder, path, onClose, onMoved }: { folder: string; path: string; onClose: () => void; onMoved: () => void }) {
  const t = useT()
  const refresh = useRefreshLibrary()
  const [to, setTo] = useState(path)
  const move = useApiMutation(() => api.post<{ moved: number }>('/api/files/move', { folder, from: path, to }), {
    onSuccess: (r) => {
      toast.success(t('Moved {count} file(s)', { count: r.moved }))
      void refresh()
      onMoved()
    },
  })
  return (
    <FormDialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={t('Move or rename')}
      description={t('Type the new path on {folder}. Folders are made as needed. Albums that show the folder follow it.', { folder })}
      onSubmit={() => move.mutate()}
      submit={t('Move')}
      disabled={!to.trim() || to.trim() === path}
      pending={move.isPending}
    >
      <Input autoFocus value={to} onChange={(e) => setTo(e.target.value)} aria-label={t('New path')} />
    </FormDialog>
  )
}
