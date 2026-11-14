import { useApiMutation } from '@/lib/mutation'
import { useState } from 'react'
import { toast } from 'sonner'
import { FormDialog } from '@/components/form-dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { changePassword } from '@/services/session'
import { useT } from '@/lib/i18n'

/** Everybody changes their own password here. Their other logins end; this one stays. */
export function ChangePassword({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [again, setAgain] = useState('')
  const change = useApiMutation(() => changePassword(current, next), {
    inline: true,
    onSuccess: () => {
      toast.success(t('Password changed. Your other logins have ended.'))
      close()
    },
  })
  const close = () => {
    setCurrent('')
    setNext('')
    setAgain('')
    change.reset()
    onClose()
  }
  const mismatch = again !== '' && again !== next
  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { if (!o) close() }}
      title={t('Change password')}
      description={t('At least 8 characters.')}
      onSubmit={() => change.mutate()}
      submit={t('Change password')}
      disabled={!current || next.length < 8 || next !== again}
      pending={change.isPending}
      error={change.error?.message}
    >
      <div className="space-y-2">
        <Label htmlFor="current-password">{t('Current password')}</Label>
        <Input id="current-password" type="password" autoComplete="current-password" autoFocus value={current} onChange={(e) => setCurrent(e.target.value)} />
      </div>
      <div className="space-y-2">
        <Label htmlFor="new-password">{t('New password')}</Label>
        <Input id="new-password" type="password" autoComplete="new-password" value={next} onChange={(e) => setNext(e.target.value)} />
      </div>
      <div className="space-y-2">
        <Label htmlFor="repeat-password">{t('New password again')}</Label>
        <Input id="repeat-password" type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} />
        {mismatch && <p className="text-sm text-destructive">{t('The two passwords differ.')}</p>}
      </div>
    </FormDialog>
  )
}
