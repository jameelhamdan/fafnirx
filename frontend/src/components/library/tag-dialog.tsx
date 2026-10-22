import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { plural } from '@/lib/format'
import { useLibraryActions, useTags } from '@/lib/library'

/** Adds tags to many files at once. Tags they already have are kept. */
export function TagDialog({ open, ids, onClose, onDone }: { open: boolean; ids: number[]; onClose: () => void; onDone?: () => void }) {
  const { bulk } = useLibraryActions()
  const tags = useTags(open)
  const [text, setText] = useState('')
  const names = text.split(',').map((t) => t.trim()).filter(Boolean)
  const add = (name: string) => setText((t) => (t.trim() ? `${t.trim().replace(/,$/, '')}, ${name}` : name))

  const submit = (e: FormEvent) => {
    e.preventDefault()
    if (names.length === 0) return
    bulk.mutate({ ids, add_tags: names }, {
      onSuccess: () => {
        toast.success(`Tagged ${plural(ids.length, 'file')}`)
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
            <DialogTitle>Add tags</DialogTitle>
            <DialogDescription>To {plural(ids.length, 'photo or video')}. Separate several tags with commas.</DialogDescription>
          </DialogHeader>
          <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder="holiday, family" aria-label="Tags" />
          {tags.data && tags.data.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {tags.data.slice(0, 30).map((t) => (
                <button key={t.id} type="button" onClick={() => add(t.name)} className="rounded-full border px-2.5 py-0.5 text-xs hover:bg-accent">{t.name}</button>
              ))}
            </div>
          )}
          <DialogFooter><Button type="submit" disabled={names.length === 0 || bulk.isPending}>Add</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
