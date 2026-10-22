import { useQueryClient } from '@tanstack/react-query'
import { AlertCircle, Check, ChevronDown, ChevronUp, Loader2, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { bytes, percent, plural } from '@/lib/format'
import { clearFinishedUploads, setUploadsDoneHandler, useUploads } from '@/lib/upload'
import { Button } from '../ui/button'
import { Progress } from '../ui/progress'
import { useT } from '@/lib/i18n'

/** Progress of the uploads started in this browser, as a card in the corner. */
export function UploadPanel() {
  const client = useQueryClient()
  const items = useUploads()
  const t = useT()
  const [open, setOpen] = useState(true)

  useEffect(() => {
    setUploadsDoneHandler(() => void client.invalidateQueries({ queryKey: ['files'] }))
  }, [client])

  if (items.length === 0) return null
  const active = items.filter((i) => i.status === 'queued' || i.status === 'uploading')
  const failed = items.filter((i) => i.status === 'error').length
  const total = items.reduce((n, i) => n + i.size, 0)
  const sent = items.reduce((n, i) => n + (i.status === 'done' ? i.size : i.sent), 0)

  return (
    <div className="fixed right-3 bottom-3 z-40 w-[min(22rem,calc(100vw-1.5rem))] rounded-lg border bg-popover text-popover-foreground shadow-lg">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        {active.length > 0 ? <Loader2 className="size-4 animate-spin text-primary" /> : failed ? <AlertCircle className="size-4 text-destructive" /> : <Check className="size-4 text-success" />}
        <p className="flex-1 text-sm font-medium">
          {active.length > 0 ? t('Uploading {files}', { files: plural(active.length, 'file') }) : failed ? t('{count} failed', { count: failed }) : t('Upload complete')}
        </p>
        <Button variant="ghost" size="icon" className="size-7" aria-label={open ? t('Collapse') : t('Expand')} onClick={() => setOpen(!open)}>
          {open ? <ChevronDown className="size-4" /> : <ChevronUp className="size-4" />}
        </Button>
        {active.length === 0 && (
          <Button variant="ghost" size="icon" className="size-7" aria-label={t('Dismiss')} onClick={clearFinishedUploads}>
            <X className="size-4" />
          </Button>
        )}
      </div>
      {active.length > 0 && <Progress value={percent(sent, total) ?? 0} className="h-1 rounded-none" />}
      {open && (
        <ul className="max-h-64 divide-y overflow-y-auto">
          {items.map((i) => (
            <li key={i.id} className="space-y-1 px-3 py-2 text-xs">
              <div className="flex justify-between gap-2">
                <span className="truncate" title={`${i.device}/${i.path}`}>{i.path}</span>
                <span className="shrink-0 text-muted-foreground">
                  {i.status === 'done' ? t('Done') : i.status === 'error' ? t('Failed') : i.status === 'queued' ? t('Waiting') : `${percent(i.sent, i.size) ?? 0}%`}
                </span>
              </div>
              {i.status === 'uploading' && <Progress value={percent(i.sent, i.size) ?? 0} className="h-1" />}
              {i.status === 'error' && <p className="text-destructive">{i.error}</p>}
              {i.status === 'queued' && <p className="text-muted-foreground">{bytes(i.size)}</p>}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
