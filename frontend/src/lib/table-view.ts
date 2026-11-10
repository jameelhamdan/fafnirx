import { useCallback, useMemo, useState } from 'react'

export type Density = 'compact' | 'normal' | 'comfortable'
const DENSITIES: Density[] = ['compact', 'normal', 'comfortable']
export type Column = { id: string; label: string; locked?: boolean }

type Saved = { density: Density; hidden: string[] }

function load(id: string): Saved {
  try {
    const raw = JSON.parse(localStorage.getItem(`fafnirx.table.${id}`) ?? 'null') as Partial<Saved> | null
    return { density: DENSITIES.includes(raw?.density as Density) ? (raw!.density as Density) : 'normal', hidden: Array.isArray(raw?.hidden) ? raw!.hidden : [] }
  } catch {
    return { density: 'normal', hidden: [] }
  }
}

/** How one table is shown: its density and the columns the person hid. Remembered in this browser, per table. */
export function useTableView(id: string, columns: Column[]) {
  const [saved, setSaved] = useState<Saved>(() => load(id))
  const update = useCallback(
    (next: Saved) => {
      setSaved(next)
      try {
        localStorage.setItem(`fafnirx.table.${id}`, JSON.stringify(next))
      } catch {
        // Private window or blocked storage: the choice lasts until the page is closed.
      }
    },
    [id],
  )
  return useMemo(
    () => ({
      columns,
      density: saved.density,
      cycleDensity: () => update({ ...saved, density: DENSITIES[(DENSITIES.indexOf(saved.density) + 1) % DENSITIES.length] }),
      show: (column: string) => !saved.hidden.includes(column),
      toggle: (column: string) => update({ ...saved, hidden: saved.hidden.includes(column) ? saved.hidden.filter((c) => c !== column) : [...saved.hidden, column] }),
    }),
    [columns, saved, update],
  )
}
export type TableView = ReturnType<typeof useTableView>

/** The rows ticked in a table, by key. */
export function useSelection(keys: string[]) {
  const [picked, setPicked] = useState<Set<string>>(new Set())
  // Rows that left the list (filtered out, finished) are no longer selected.
  const selected = useMemo(() => new Set(keys.filter((k) => picked.has(k))), [keys, picked])
  const all = keys.length > 0 && selected.size === keys.length
  return {
    selected,
    all,
    some: selected.size > 0 && !all,
    toggle: (key: string) => setPicked((p) => { const n = new Set(p); if (!n.delete(key)) n.add(key); return n }),
    toggleAll: () => setPicked(all ? new Set() : new Set(keys)),
    clear: () => setPicked(new Set()),
  }
}
export type Selection = ReturnType<typeof useSelection>


/** True when a row's text contains every word of the filter, whatever the case. */
export function matches(filter: string, ...fields: (string | null | undefined)[]): boolean {
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean)
  if (words.length === 0) return true
  const text = fields.join(' ').toLowerCase()
  return words.every((w) => text.includes(w))
}
