import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import { useRefreshLibrary } from '@/lib/library'

/**
 * Renames or moves a file, or a folder with what is in it, within a device the master owns (a local or shared folder, or the
 * master files). The new place is a full path on the same device; folders in it are made as needed.
 */
export function MoveDialog({ device, path, onClose, onMoved }: { device: string; path: string; onClose: () => void; onMoved: () => void }) {
  const t = useT()
  const client = useQueryClient()
  const refresh = useRefreshLibrary()
  const [to, setTo] = useState(path)
  const move = useMutation({
    mutationFn: () => api.post<{ moved: number }>('/api/files/move', { device, from: path, to }),
    onSuccess: (r) => {
      toast.success(t('Moved {count} file(s)', { count: r.moved }))
      void client.invalidateQueries({ queryKey: ['files'] })
      void refresh()
      onMoved()
    },
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (to.trim() && to.trim() !== path) move.mutate()
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Move or rename')}</DialogTitle>
            <DialogDescription>{t('Type the new path on {device}. Folders are made as needed. Albums that show the folder follow it.', { device })}</DialogDescription>
          </DialogHeader>
          <Input autoFocus value={to} onChange={(e) => setTo(e.target.value)} aria-label={t('New path')} />
          <DialogFooter><Button type="submit" disabled={!to.trim() || to.trim() === path || move.isPending}>{t('Move')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
