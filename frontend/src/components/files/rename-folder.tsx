import { useQueryClient } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { useState } from 'react'
import { FormDialog } from '../form-dialog'
import { Input } from '../ui/input'
import { Label } from '../ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'

/**
 * Gives a folder another name: one of yours (a local or shared one), or with `masterId` a master folder, whose name is its path
 * in the master files. Its files, members, albums, sources and mirrors stay with it, and the computers that use it follow the new name.
 */
export function RenameFolder({ folder, masterId, onClose, onRenamed }: { folder: string | null; masterId?: string; onClose: () => void; onRenamed: (folder: string) => void }) {
  const t = useT()
  const client = useQueryClient()
  const current = masterId ? (folder ?? '') : (folder?.split('/').pop() ?? '')
  const [name, setName] = useState('')
  const rename = useApiMutation(
    async (to: string) =>
      masterId
        ? api.post(`/api/master-folders/${masterId}/rename`, { path: to }).then(() => ({ folder: to }))
        : api.post<{ folder: string }>(`/api/devices/${encodeURIComponent(folder ?? '')}/rename`, { name: to }),
    {
      onSuccess: (done) => {
        for (const key of ['devices', 'files', 'folders', 'albums', 'album', 'sources']) void client.invalidateQueries({ queryKey: [key] })
        setName('')
        onRenamed(done.folder)
      },
    },
  )
  return (
    <FormDialog
      open={folder !== null}
      onOpenChange={(o) => { if (!o) { onClose(); rename.reset(); setName('') } }}
      title={t('Rename folder')}
      description={t('The files, the people it is shared with, its albums and its sources stay with the folder. Computers that use it follow the new name.')}
      onSubmit={() => rename.mutate(name.trim())}
      submit={t('Rename')}
      disabled={!name.trim() || name.trim() === current}
      pending={rename.isPending}
      error={rename.error?.message}
    >
      <div className="space-y-2">
        <Label htmlFor="rename-folder">{t('Name')}</Label>
        <Input id="rename-folder" autoFocus placeholder={current} value={name} onChange={(e) => setName(e.target.value)} />
        <p className="text-xs text-muted-foreground">{masterId ? t('A path in the master files, for example Music or Media/Music.') : t('Lowercase letters, digits, dot, underscore and dash.')}</p>
      </div>
    </FormDialog>
  )
}
