import { Check, Heart, Image as ImageIcon, Play } from 'lucide-react'
import { clock } from '@/lib/format'
import type { Asset } from '@/lib/types'
import { cn } from '@/lib/utils'
import { Thumb } from './thumb'

/** One square of a photo grid: the picture, a mark for videos and favorites, and a way to select it. */
export function AssetCell({
  asset,
  src,
  selected,
  selecting,
  onOpen,
  onToggle,
}: {
  asset: Asset
  src: string
  selected: boolean
  /** Something is selected: a click selects instead of opening. */
  selecting: boolean
  onOpen: () => void
  onToggle: () => void
}) {
  return (
    <div className={cn('group relative aspect-square overflow-hidden bg-muted/60', selected && 'bg-primary/20')}>
      <button
        type="button"
        onClick={() => (selecting ? onToggle() : onOpen())}
        className={cn('absolute inset-0 outline-offset-[-3px] transition-transform', selected && 'scale-90 rounded-md')}
        aria-label={`${asset.kind === 'video' ? 'Video' : 'Photo'} ${asset.name}`}
      >
        <Thumb src={src} className="size-full" fallback={<ImageIcon className="size-6 text-muted-foreground" />} />
      </button>
      {asset.kind === 'video' && (
        <span className="pointer-events-none absolute top-1.5 right-1.5 flex items-center gap-1 rounded bg-black/55 px-1.5 py-0.5 text-[11px] font-medium text-white">
          {asset.duration_ms ? clock(asset.duration_ms) : null}
          <Play className="size-3 fill-white" />
        </span>
      )}
      {asset.favorite && <Heart className="pointer-events-none absolute bottom-1.5 left-1.5 size-4 fill-white text-white drop-shadow" />}
      <button
        type="button"
        onClick={onToggle}
        aria-label={selected ? `Deselect ${asset.name}` : `Select ${asset.name}`}
        aria-pressed={selected}
        className={cn(
          'absolute top-1.5 left-1.5 grid size-5 place-items-center rounded-full border-2 border-white bg-black/30 text-white shadow transition-opacity focus-visible:opacity-100',
          selected ? 'border-primary bg-primary opacity-100' : 'opacity-0 group-hover:opacity-100 max-sm:opacity-70',
        )}
      >
        {selected && <Check className="size-3" strokeWidth={3} />}
      </button>
    </div>
  )
}
