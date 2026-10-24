import { useMutation } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'

/** Everybody changes their own password here. Their other logins end; this one stays. */
export function ChangePassword({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT()
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [again, setAgain] = useState('')
  const change = useMutation({
    mutationFn: () => api.post('/api/me/password', { current, new: next }),
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
  const submit = (e: FormEvent) => {
    e.preventDefault()
    change.mutate()
  }
  const mismatch = again !== '' && again !== next
  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) close() }}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Change password')}</DialogTitle>
            <DialogDescription>{t('At least 8 characters.')}</DialogDescription>
          </DialogHeader>
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
            {change.error && <p role="alert" className="text-sm text-destructive">{change.error.message}</p>}
          </div>
          <DialogFooter>
            <Button type="submit" disabled={!current || next.length < 8 || next !== again || change.isPending}>{t('Change password')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
