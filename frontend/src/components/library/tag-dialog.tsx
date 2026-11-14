import { useState } from 'react'
import { toast } from 'sonner'
import { FormDialog } from '../form-dialog'
import { Button } from '../ui/button'
import { Input } from '../ui/input'
import { plural } from '@/lib/format'
import { useLibraryActions, useTags } from '@/services/library'
import { useT } from '@/lib/i18n'

/** Adds tags to many files at once. Tags they already have are kept. */
export function TagDialog({ open, ids, onClose, onDone }: { open: boolean; ids: string[]; onClose: () => void; onDone?: () => void }) {
  const { bulk } = useLibraryActions()
  const t = useT()
  const tags = useTags(open)
  const [text, setText] = useState('')
  const names = text.split(',').map((s) => s.trim()).filter(Boolean)
  const add = (name: string) => setText((s) => (s.trim() ? `${s.trim().replace(/,$/, '')}, ${name}` : name))

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title={t('Add tags')}
      description={t('To {items}. Separate several tags with commas.', { items: plural(ids.length, 'photo or video') })}
      onSubmit={() =>
        bulk.mutate({ ids, add_tags: names }, {
          onSuccess: () => {
            toast.success(t('Tagged {files}', { files: plural(ids.length, 'file') }))
            setText('')
            onClose()
            onDone?.()
          },
        })
      }
      submit={t('Add')}
      disabled={names.length === 0}
      pending={bulk.isPending}
    >
      <Input autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder={t('holiday, family')} aria-label={t('Tags')} />
      {tags.data && tags.data.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {tags.data.slice(0, 30).map((tag) => (
            <Button key={tag.id} type="button" variant="outline" size="xs" onClick={() => add(tag.name)}>{tag.name}</Button>
          ))}
        </div>
      )}
    </FormDialog>
  )
}
