import { useVirtualizer } from '@tanstack/react-virtual'
import { ArrowDown, ArrowUp } from 'lucide-react'
import { useRef } from 'react'
import { ago, bytes, plural } from '@/lib/format'
import type { Entry } from '@/lib/types'
import { entryKey } from '@/lib/entries'
import { cn } from '@/lib/utils'
import { Checkbox } from '../ui/checkbox'
import { FileIcon } from './file-icon'

const COLUMNS = 'grid-cols-[2rem_minmax(0,1fr)_5rem] sm:grid-cols-[2rem_minmax(0,1fr)_6rem_9rem] lg:grid-cols-[2rem_minmax(0,1fr)_6rem_9rem_4rem]'

function SortHeader({ field, label, sort, order, onSort, className }: { field: string; label: string; sort: string; order: string; onSort: (f: string) => void; className?: string }) {
  const active = sort === field
  return (
    <button type="button" onClick={() => onSort(field)} className={cn('flex items-center gap-1 hover:text-foreground', active && 'text-foreground', className)}>
      {label}
      {active && (order === 'desc' ? <ArrowDown className="size-3" /> : <ArrowUp className="size-3" />)}
    </button>
  )
}

/** A long list that only draws the rows in view, so a folder of 100,000 photos stays quick. */
export function FileList({
  entries,
  selected,
  onToggle,
  onToggleAll,
  onOpen,
  sort,
  order,
  onSort,
  now,
}: {
  entries: Entry[]
  selected: Set<string>
  onToggle: (key: string) => void
  onToggleAll: () => void
  onOpen: (e: Entry) => void
  sort: string
  order: string
  onSort: (field: string) => void
  now: number
}) {
  const parent = useRef<HTMLDivElement>(null)
  const virtual = useVirtualizer({ count: entries.length, getScrollElement: () => parent.current, estimateSize: () => 44, overscan: 12 })
  const all = entries.length > 0 && entries.every((e) => selected.has(entryKey(e)))

  return (
    <div className="overflow-hidden rounded-lg border bg-card">
      <div className={cn('grid items-center gap-2 border-b bg-muted/40 px-3 py-2 text-xs font-medium text-muted-foreground', COLUMNS)}>
        <Checkbox checked={all} onCheckedChange={onToggleAll} aria-label="Select all" />
        <SortHeader field="name" label="Name" {...{ sort, order, onSort }} />
        <SortHeader field="size" label="Size" {...{ sort, order, onSort }} className="justify-end" />
        <SortHeader field="modified" label="Modified" {...{ sort, order, onSort }} className="hidden sm:flex" />
        <span className="hidden text-right lg:block">Versions</span>
      </div>
      <div ref={parent} className="h-[calc(100svh-22rem)] min-h-72 overflow-y-auto">
        <div className="relative w-full" style={{ height: virtual.getTotalSize() }}>
          {virtual.getVirtualItems().map((row) => {
            const e = entries[row.index]
            const key = entryKey(e)
            const on = selected.has(key)
            const gone = e.deleted_at !== null
            return (
              <div
                key={key}
                role="row"
                aria-selected={on}
                className={cn('absolute top-0 left-0 grid w-full items-center gap-2 border-b px-3 text-sm', COLUMNS, on ? 'bg-accent' : 'hover:bg-muted/50')}
                style={{ height: row.size, transform: `translateY(${row.start}px)` }}
              >
                <Checkbox checked={on} onCheckedChange={() => onToggle(key)} aria-label={`Select ${e.name}`} />
                <button type="button" onClick={() => onOpen(e)} className="flex min-w-0 items-center gap-2.5 text-left" title={e.name}>
                  <FileIcon entry={e} />
                  <span className={cn('truncate', e.kind !== 'file' && 'font-medium', gone && 'text-muted-foreground line-through')}>{e.name}</span>
                  {gone && e.kind === 'file' && <span className="shrink-0 text-xs text-destructive">deleted</span>}
                  {e.kind !== 'file' && <span className="shrink-0 text-xs text-muted-foreground">{plural(e.files ?? 0, 'file')}</span>}
                </button>
                <span className="text-right text-muted-foreground tabular-nums" title={e.size === null ? undefined : `${e.size.toLocaleString()} bytes`}>
                  {bytes(e.size)}
                </span>
                <span className="hidden text-muted-foreground sm:block">{e.kind === 'file' ? ago(e.mtime ?? e.changed, now) : e.changed ? ago(e.changed, now) : ''}</span>
                <span className="hidden text-right text-muted-foreground tabular-nums lg:block">{e.kind === 'file' && (e.versions ?? 0) > 1 ? e.versions : ''}</span>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
