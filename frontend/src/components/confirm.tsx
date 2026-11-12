import { Button } from './ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from './ui/dialog'
import { useT } from '@/lib/i18n'

/** A yes/no question before something that cannot be undone. It closes (`onClose`) on either answer. */
export function Confirm({
  open,
  title,
  description,
  action,
  destructive = false,
  onConfirm,
  onClose,
}: {
  open: boolean
  title: string
  description: string
  action: string
  destructive?: boolean
  onConfirm: () => void
  onClose: () => void
}) {
  const t = useT()
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t('Cancel')}</Button>
          <Button variant={destructive ? 'destructive' : 'default'} onClick={() => { onConfirm(); onClose() }}>{action}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
