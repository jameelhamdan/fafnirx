import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { plural } from '@/lib/format'
import { useLibraryActions, useTags } from '@/lib/library'
import { useT } from '@/lib/i18n'

/** Adds tags to many files at once. Tags they already have are kept. */
export function TagDialog({ open, ids, onClose, onDone }: { open: boolean; ids: number[]; onClose: () => void; onDone?: () => void }) {
  const { bulk } = useLibraryActions()
  const t = useT()
  const tags = useTags(open)
  const [text, setText] = useState('')
  const names = text.split(',').map((s) => s.trim()).filter(Boolean)
  const add = (name: string) => setText((s) => (s.trim() ? `${s.trim().replace(/,$/, '')}, ${name}` : name))

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (names.length === 0) return
    bulk.mutate({ ids, add_tags: names }, {
      onSuccess: () => {
        toast.success(t('Tagged {files}', { files: plural(ids.length, 'file') }))
        setText('')
        onClose()
        onDone?.()
      },
    })
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Add tags')}</DialogTitle>
            <DialogDescription>{t('To {items}. Separate several tags with commas.', { items: plural(ids.length, 'photo or video') })}</DialogDescription>
          </DialogHeader>
          <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder={t('holiday, family')} aria-label={t('Tags')} />
          {tags.data && tags.data.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {tags.data.slice(0, 30).map((tag) => (
                <button key={tag.id} type="button" onClick={() => add(tag.name)} className="rounded-full border px-2.5 py-0.5 text-xs hover:bg-accent">{tag.name}</button>
              ))}
            </div>
          )}
          <DialogFooter><Button type="submit" disabled={names.length === 0 || bulk.isPending}>{t('Add')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
