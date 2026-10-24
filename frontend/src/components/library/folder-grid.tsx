import { useRef } from 'react'
import { GAP, useGrid } from '@/lib/grid'
import type { Asset } from '@/lib/types'
import { AssetCell } from './asset-cell'

/** The folder an asset is in, below the album's folder (`Season 1` for `Show/Season 1/Episode 2.mkv` in the album `Show`). */
const subfolder = (asset: Asset, root: string) => {
  const below = root && asset.path.startsWith(root + '/') ? asset.path.slice(root.length + 1) : asset.path
  return below.includes('/') ? below.slice(0, below.lastIndexOf('/')) : ''
}

/** A file's name without its extension, for a caption. */
const title = (name: string) => (name.includes('.') ? name.slice(0, name.lastIndexOf('.')) : name)

/**
 * The files of an album that shows a folder, in the order they come (the order of their names), under a heading for each
 * subfolder: the seasons of a show. Each has its name under it, because the names are what tell episodes apart.
 */
export function FolderGrid({ assets, root, src, selected, onOpen, onToggle }: {
  assets: Asset[]
  /** The path of the album's folder inside its device. */
  root: string
  src: (a: Asset) => string
  selected: Set<number>
  onOpen: (a: Asset) => void
  onToggle: (id: number) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const { cols } = useGrid(ref)
  const selecting = selected.size > 0
  // Neighbours in the same folder make one group; the order is not changed.
  const groups: { folder: string; assets: Asset[] }[] = []
  for (const asset of assets) {
    const folder = subfolder(asset, root)
    const last = groups[groups.length - 1]
    if (last && last.folder === folder) last.assets.push(asset)
    else groups.push({ folder, assets: [asset] })
  }
  return (
    <div ref={ref}>
      {groups.map(({ folder, assets: group }, i) => (
        <section key={`${i}-${folder}`} className="mb-4" aria-label={folder || undefined}>
          {folder && <h3 className="mb-1.5 text-sm font-medium">{folder}</h3>}
          <div className="grid" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, gap: GAP }}>
            {group.map((a) => (
              <div key={a.id} className="min-w-0">
                <AssetCell asset={a} src={src(a)} selected={selected.has(a.id)} selecting={selecting} onOpen={() => onOpen(a)} onToggle={() => onToggle(a.id)} />
                <p className="mt-1 truncate text-xs text-muted-foreground" title={a.name}>{title(a.name)}</p>
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}
