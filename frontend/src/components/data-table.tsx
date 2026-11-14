import { AlignJustify, Columns3, Search, X } from 'lucide-react'
import { Panel } from './common'
import type { ReactNode } from 'react'
import { msg, useT } from '@/lib/i18n'
import type { Density, Selection, TableView } from '@/lib/table-view'
import { cn, HIT_AREA } from '@/lib/utils'
import { Button } from './ui/button'
import { Checkbox } from './ui/checkbox'
import { DropdownMenu, DropdownMenuCheckboxItem, DropdownMenuContent, DropdownMenuLabel, DropdownMenuTrigger } from './ui/dropdown-menu'
import { Input } from './ui/input'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

/** The checkbox in a header cell that selects every row. */
export function SelectAll({ selection }: { selection: Selection }) {
  const t = useT()
  return <Checkbox checked={selection.all ? true : selection.some ? 'indeterminate' : false} onCheckedChange={selection.toggleAll} aria-label={t('Select all rows')} className={HIT_AREA} />
}

/** The checkbox of one row. */
export function SelectRow({ selection, id }: { selection: Selection; id: string }) {
  const t = useT()
  return <Checkbox checked={selection.selected.has(id)} onCheckedChange={() => selection.toggle(id)} aria-label={t('Select row')} className={HIT_AREA} />
}

const DENSITY_LABEL: Record<Density, string> = { compact: msg('Compact rows'), normal: msg('Normal rows'), comfortable: msg('Comfortable rows') }

/**
 * The strip above a table: a filter box, the row count, what to do with the selected rows (`actions`),
 * the density toggle and the menu of columns.
 */
export function TableToolbar({
  view,
  filter,
  onFilter,
  placeholder,
  shown,
  total,
  selection,
  actions,
  children,
}: {
  view: TableView
  filter: string
  onFilter: (value: string) => void
  placeholder: string
  shown: number
  total: number
  selection?: Selection
  actions?: ReactNode
  children?: ReactNode
}) {
  const t = useT()
  const picked = selection?.selected.size ?? 0
  const optional = view.columns.filter((c) => !c.locked)
  return (
    <div className="flex min-h-10 flex-wrap items-center gap-2 border-b px-3 py-2">
      <div className="relative w-64 max-w-full">
        <Search className="pointer-events-none absolute start-2 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input className="h-8 ps-8 pe-7 text-compact" value={filter} placeholder={placeholder} aria-label={placeholder} onChange={(e) => onFilter(e.target.value)} />
        {filter && (
          <Button type="button" variant="ghost" size="icon-xs" className="absolute end-1 top-1/2 -translate-y-1/2 text-muted-foreground" aria-label={t('Clear filter')} onClick={() => onFilter('')}>
            <X />
          </Button>
        )}
      </div>
      {children}
      <span className="num text-xs text-muted-foreground">{shown === total ? total : `${shown} / ${total}`}</span>
      {picked > 0 && (
        <div className="flex items-center gap-1.5 border-s ps-2">
          <span className="num text-xs">{t('{n} selected', { n: picked })}</span>
          {actions}
          <Button size="xs" variant="ghost" onClick={selection?.clear}>{t('Clear')}</Button>
        </div>
      )}
      <div className="ms-auto flex items-center gap-0.5">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button size="icon-sm" variant="ghost" onClick={view.cycleDensity} aria-label={t(DENSITY_LABEL[view.density])}>
              <AlignJustify className={cn('size-4', view.density === 'compact' && 'scale-y-75', view.density === 'comfortable' && 'scale-y-125')} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t(DENSITY_LABEL[view.density])}</TooltipContent>
        </Tooltip>
        {optional.length > 0 && (
          <DropdownMenu>
            <Tooltip>
              <TooltipTrigger asChild>
                <DropdownMenuTrigger asChild>
                  <Button size="icon-sm" variant="ghost" aria-label={t('Columns')}><Columns3 className="size-4" /></Button>
                </DropdownMenuTrigger>
              </TooltipTrigger>
              <TooltipContent>{t('Columns')}</TooltipContent>
            </Tooltip>
            <DropdownMenuContent align="end">
              <DropdownMenuLabel>{t('Columns')}</DropdownMenuLabel>
              {optional.map((c) => (
                <DropdownMenuCheckboxItem key={c.id} checked={view.show(c.id)} onSelect={(e) => e.preventDefault()} onCheckedChange={() => view.toggle(c.id)}>
                  {c.label}
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
    </div>
  )
}

/** A table with its toolbar in one bordered frame; the density applies to the rows inside. */
export function DataTable({ view, toolbar, children, className }: { view: TableView; toolbar: ReactNode; children: ReactNode; className?: string }) {
  return (
    <Panel data-density={view.density} className={className}>
      {toolbar}
      {children}
    </Panel>
  )
}

