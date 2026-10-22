import { useRef } from 'react'
import { dayTitle } from '@/lib/format'
import { GAP, groupByDay, useGrid } from '@/lib/grid'
import type { Asset } from '@/lib/types'
import { AssetCell } from './asset-cell'
import { Checkbox } from '../ui/checkbox'
import { useT } from '@/lib/i18n'

/** Photos grouped by day, each day with its own heading, in square cells that fill the width. */
export function DayGrid({
  assets,
  src,
  selected,
  onOpen,
  onToggle,
  onToggleMany,
}: {
  assets: Asset[]
  src: (a: Asset) => string
  selected: Set<number>
  onOpen: (a: Asset) => void
  onToggle: (id: number) => void
  onToggleMany: (ids: number[], on: boolean) => void
}) {
  const t = useT()
  const ref = useRef<HTMLDivElement>(null)
  const { cols } = useGrid(ref)
  const selecting = selected.size > 0
  return (
    <div ref={ref}>
      {groupByDay(assets).map(({ day, assets: group }) => {
        const ids = group.map((a) => a.id)
        const all = ids.every((id) => selected.has(id))
        return (
          <section key={day} className="mb-4" aria-label={dayTitle(day)}>
            <h3 className="group mb-1.5 flex items-center gap-2 text-sm font-medium">
              {dayTitle(day)}
              <span className="text-xs font-normal text-muted-foreground">{group.length}</span>
              <Checkbox
                checked={all}
                onCheckedChange={() => onToggleMany(ids, !all)}
                aria-label={t('Select all of {day}', { day: dayTitle(day) })}
                className={selecting ? '' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100 max-sm:opacity-60'}
              />
            </h3>
            <div className="grid" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, gap: GAP }}>
              {group.map((a) => (
                <AssetCell key={a.id} asset={a} src={src(a)} selected={selected.has(a.id)} selecting={selecting} onOpen={() => onOpen(a)} onToggle={() => onToggle(a.id)} />
              ))}
            </div>
          </section>
        )
      })}
    </div>
  )
}
