import { thumbUrl } from '@/lib/api'
import { bytes, plural } from '@/lib/format'
import { entryKey } from '@/lib/entries'
import type { Entry } from '@/lib/types'
import { cn } from '@/lib/utils'
import { Thumb } from '../library/thumb'
import { Checkbox } from '../ui/checkbox'
import { AccessBadge } from './access-badge'
import { FileIcon } from './file-icon'
import { useT } from '@/lib/i18n'

/** Thumbnails for photos, icons for everything else. */
export function FileGrid({
  entries,
  selected,
  onToggle,
  onOpen,
}: {
  entries: Entry[]
  selected: Set<string>
  onToggle: (key: string) => void
  onOpen: (e: Entry) => void
}) {
  const t = useT()
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6">
      {entries.map((e) => {
        const key = entryKey(e)
        const on = selected.has(key)
        return (
          <div key={key} className={cn('group relative overflow-hidden rounded-lg border bg-card transition-colors', on && 'border-primary ring-2 ring-primary/40')}>
            <button type="button" onClick={() => onOpen(e)} className="block w-full text-left" title={e.name}>
              <div className="grid aspect-square place-items-center bg-muted/40">
                {e.kind === 'file' && e.preview ? (
                  <Thumb src={thumbUrl(e.device, e.path)} className={cn('size-full', e.deleted_at && 'opacity-40')} fallback={<FileIcon entry={e} className="size-10" />} />
                ) : (
                  <FileIcon entry={e} className="size-10" />
                )}
              </div>
              <div className="px-2.5 py-2">
                <p className={cn('truncate text-sm', e.kind !== 'file' && 'font-medium')}>{e.name}</p>
                <p className="text-xs text-muted-foreground">{e.kind === 'file' ? bytes(e.size) : `${plural(e.files ?? 0, 'file')} · ${bytes(e.size)}`}</p>
                {e.kind === 'device' && <div className="mt-1"><AccessBadge writable={e.writable} kind={e.device_kind} /></div>}
              </div>
            </button>
            <Checkbox
              checked={on}
              onCheckedChange={() => onToggle(key)}
              aria-label={t('Select {name}', { name: e.name })}
              className={cn('absolute top-2 left-2 bg-background/90', !on && 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100 max-sm:opacity-100')}
            />
          </div>
        )
      })}
    </div>
  )
}
