import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { FormDialog } from '@/components/form-dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'

/**
 * Makes a folder of the user's: it lives on the master, anything can be uploaded into it, and computers can keep it in step. The
 * dialog opens and closes where it is used (`onCreated` gets the folder as clients name it).
 */
export function NewFolder({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: (folder: string) => void }) {
  const t = useT()
  const client = useQueryClient()
  const me = useSession()?.user?.name
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post<{ folder: string }>('/api/folders', { name }),
    onSuccess: (made) => {
      for (const key of ['folders', 'devices', 'files']) void client.invalidateQueries({ queryKey: [key] })
      setName('')
      onCreated(made.folder)
    },
  })
  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { if (!o) { onClose(); create.reset() } }}
      title={t('New folder')}
      description={t('A folder of yours that lives on the master. Upload into it, or copy files straight into files/<you>/<name>/ on the server; your computers can keep it in step.')}
      onSubmit={() => create.mutate()}
      submit={t('Create')}
      disabled={!name.trim()}
      pending={create.isPending}
      error={create.error?.message}
    >
      <div className="space-y-2">
        <Label htmlFor="folder-name">{t('Name')}</Label>
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">{me}/</span>
          <Input id="folder-name" autoFocus placeholder="photos" value={name} onChange={(e) => setName(e.target.value)} />
        </div>
        <p className="text-xs text-muted-foreground">{t('Lowercase letters, digits, dot, underscore and dash; up to 63 characters.')}</p>
      </div>
    </FormDialog>
  )
}
