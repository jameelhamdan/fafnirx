import { Plus } from 'lucide-react'
import { useState } from 'react'
import { FormDialog } from '@/components/form-dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import { useApiMutation } from '@/lib/mutation'

/** The dialog an administrator adds a master folder with. */
export function AddMasterFolder() {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [path, setPath] = useState('')
  const create = useApiMutation(() => api.post('/api/master-folders', { path }), {
    invalidate: [['folders']],
    onSuccess: () => {
      setOpen(false)
      setPath('')
    },
  })
  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { setOpen(o); create.reset() }}
      trigger={<Button size="sm" variant="outline"><Plus className="size-4" />{t('Add a master folder')}</Button>}
      title={t('Add a master folder')}
      description={t('A folder inside the master files that computers can keep in step both ways. It is made if it does not exist. It cannot be inside another master folder or hold one.')}
      onSubmit={() => create.mutate()}
      submit={t('Add')}
      disabled={!path.trim()}
      pending={create.isPending}
      error={create.error?.message}
    >
      <div className="space-y-1.5">
        <Label htmlFor="mf-path">{t('Folder in the master files')}</Label>
        <Input id="mf-path" value={path} placeholder="Music" onChange={(e) => setPath(e.target.value)} autoFocus />
      </div>
    </FormDialog>
  )
}
