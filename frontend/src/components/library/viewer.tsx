import { Archive, ArchiveRestore, ChevronLeft, ChevronRight, Download, FileImage, FolderPlus, Heart, Info, MoreVertical, Trash2, X } from 'lucide-react'
import { lazy, Suspense, useEffect, useRef, useState } from 'react'
import { Button } from '../ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../ui/dropdown-menu'
import { Skeleton } from '../ui/skeleton'
import { Confirm } from '../confirm'
import { mediaUrl } from '@/lib/api'
import { whenText } from '@/lib/format'
import { useLibraryActions } from '@/lib/library'
import type { Asset } from '@/lib/types'
import { AlbumPicker } from './album-picker'

const InfoPanel = lazy(() => import('./info-panel'))

/** True when the key press belongs to something else (a field, a menu, a dialog on top). */
function busyElsewhere(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null
  return !!target?.closest('input, textarea, select, [contenteditable="true"]') || !!document.querySelector('[data-slot="dialog-content"], [role="menu"]')
}

function Unavailable({ asset, video, retry }: { asset: Asset; video?: boolean; retry?: () => void }) {
  return (
    <div className="flex max-w-sm flex-col items-center gap-3 p-6 text-center text-white/90">
      <FileImage className="size-10 text-white/60" />
      <p className="text-sm">
        {video
          ? 'This video cannot be played in the browser yet. A playable copy is made in the background for formats browsers do not support.'
          : 'No picture could be made of this file.'}
      </p>
      <div className="flex gap-2">
        {retry && <Button variant="secondary" size="sm" onClick={retry}>Try again</Button>}
        <Button variant="secondary" size="sm" asChild><a href={mediaUrl(asset.id, 'download', asset.hash)}><Download className="size-4" />Download the original</a></Button>
      </div>
    </div>
  )
}

function Media({ asset }: { asset: Asset }) {
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)
  if (failed) return <Unavailable asset={asset} video={asset.kind === 'video'} retry={() => { setFailed(false); setAttempt((n) => n + 1) }} />
  if (asset.kind === 'video') {
    return <video key={attempt} src={mediaUrl(asset.id, 'video', asset.hash)} poster={mediaUrl(asset.id, 'preview', asset.hash)} controls autoPlay playsInline onError={() => setFailed(true)} className="max-h-full max-w-full" />
  }
  return <img key={attempt} src={mediaUrl(asset.id, 'preview', asset.hash)} alt={asset.name} draggable={false} onError={() => setFailed(true)} className="max-h-full max-w-full object-contain select-none" />
}

/**
 * Full-screen look at one photo or video, with the ones around it a swipe or an arrow key away, and what can be done
 * with it (favorite, archive, delete, albums, info).
 */
export function Viewer({
  assets,
  id,
  onChange,
  onClose,
  onEdge,
}: {
  assets: Asset[]
  id: number
  onChange: (id: number) => void
  onClose: () => void
  /** Called when the viewer gets near the end of what it was given, so that the page can load more. */
  onEdge?: (direction: 'prev' | 'next') => void
}) {
  const { edit, remove } = useLibraryActions()
  const [info, setInfo] = useState(() => window.matchMedia('(min-width: 1024px)').matches)
  const [dialog, setDialog] = useState<'album' | 'delete' | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const index = assets.findIndex((a) => a.id === id)
  const [lastIndex, setLastIndex] = useState(Math.max(0, index))
  // A file whose date was changed leaves the month it was listed in, but you are still looking at it: the
  // viewer keeps what it last showed until you move on.
  const found = assets[index]
  const [kept, setKept] = useState<Asset | null>(null)
  const asset = found ?? kept
  // Remembered while rendering (not in an effect), so that the viewer never shows a stale picture for a frame.
  if (found && found !== kept) setKept(found)
  if (index >= 0 && index !== lastIndex) setLastIndex(index)

  useEffect(() => {
    if (!asset) onClose()
  }, [asset, onClose])
  useEffect(() => {
    if (index < 0) return
    if (index <= 2) onEdge?.('prev')
    if (index >= assets.length - 3) onEdge?.('next')
  }, [index, assets.length, onEdge])

  // Neighbours: of the file in the list, or of the place where the file was.
  const before = index >= 0 ? assets[index - 1] : assets[lastIndex - 1]
  const after = index >= 0 ? assets[index + 1] : assets[lastIndex]
  const go = (delta: number) => {
    const next = delta < 0 ? before : after
    if (next) onChange(next.id)
  }
  /** After deleting or archiving: carry on with a neighbour, or leave when nothing is left. */
  const moveOn = () => {
    const next = after ?? before
    if (next) onChange(next.id)
    else onClose()
  }
  const keys = useRef({ go, onClose, toggleInfo: () => setInfo((v) => !v), favorite: () => asset && edit.mutate({ id: asset.id, favorite: !asset.favorite }) })
  useEffect(() => {
    keys.current = { go, onClose, toggleInfo: () => setInfo((v) => !v), favorite: () => asset && edit.mutate({ id: asset.id, favorite: !asset.favorite }) }
  })
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || busyElsewhere(e)) return
      if (e.key === 'Escape') keys.current.onClose()
      else if (e.key === 'ArrowLeft') keys.current.go(-1)
      else if (e.key === 'ArrowRight') keys.current.go(1)
      else if (e.key === 'i') keys.current.toggleInfo()
      else if (e.key === 'f') keys.current.favorite()
      else return
      e.preventDefault()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // The page behind does not scroll; focus moves in and comes back.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null
    const overflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    root.current?.focus()
    return () => {
      document.body.style.overflow = overflow
      before?.focus?.()
    }
  }, [])
  // The neighbours' pictures are fetched in advance, so that moving on is instant.
  useEffect(() => {
    for (const n of [assets[index + 1], assets[index - 1]]) if (n && n.kind === 'image') new Image().src = mediaUrl(n.id, 'preview', n.hash)
  }, [assets, index])

  const touch = useRef<number | null>(null)
  if (!asset) return null

  return (
    <div
      ref={root}
      role="dialog"
      aria-modal="true"
      aria-label={asset.name}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex flex-col bg-black text-white outline-none"
    >
      <header className="z-10 flex items-center gap-1 bg-gradient-to-b from-black/70 to-transparent p-2">
        <Button variant="ghost" size="icon" className="text-white hover:bg-white/15 hover:text-white" aria-label="Close" onClick={onClose}><X className="size-5" /></Button>
        <div className="mr-auto min-w-0 px-2">
          <p className="truncate text-sm font-medium">{asset.name}</p>
          <p className="text-xs text-white/70">{whenText(asset.when)}</p>
        </div>
        <Button variant="ghost" size="icon" className="text-white hover:bg-white/15 hover:text-white" aria-label={asset.favorite ? 'Remove from favorites' : 'Add to favorites'} aria-pressed={asset.favorite} onClick={() => edit.mutate({ id: asset.id, favorite: !asset.favorite })}>
          <Heart className={asset.favorite ? 'size-5 fill-current' : 'size-5'} />
        </Button>
        <Button variant="ghost" size="icon" className="text-white hover:bg-white/15 hover:text-white" aria-label="Info" aria-pressed={info} onClick={() => setInfo((v) => !v)}><Info className="size-5" /></Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="text-white hover:bg-white/15 hover:text-white" aria-label="More actions"><MoreVertical className="size-5" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => setDialog('album')}><FolderPlus className="size-4" />Add to album</DropdownMenuItem>
            <DropdownMenuItem asChild><a href={mediaUrl(asset.id, 'download', asset.hash)}><Download className="size-4" />Download</a></DropdownMenuItem>
            <DropdownMenuItem onSelect={() => edit.mutate({ id: asset.id, archived: !asset.archived }, { onSuccess: moveOn })}>
              {asset.archived ? <ArchiveRestore className="size-4" /> : <Archive className="size-4" />}{asset.archived ? 'Unarchive' : 'Archive'}
            </DropdownMenuItem>
            <DropdownMenuSeparator />
            <DropdownMenuItem variant="destructive" onSelect={() => setDialog('delete')}><Trash2 className="size-4" />Delete</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      <div className="relative flex min-h-0 flex-1">
        <div
          className="relative grid min-w-0 flex-1 place-items-center overflow-hidden"
          onTouchStart={(e) => { touch.current = e.touches.length === 1 ? e.touches[0].clientX : null }}
          onTouchEnd={(e) => {
            const start = touch.current
            touch.current = null
            if (start === null) return
            const dx = e.changedTouches[0].clientX - start
            if (Math.abs(dx) > 70) go(dx > 0 ? -1 : 1)
          }}
        >
          <Media key={asset.id} asset={asset} />
          {before && (
            <Button variant="ghost" size="icon" className="absolute top-1/2 left-2 size-11 -translate-y-1/2 rounded-full bg-black/40 text-white hover:bg-black/60 hover:text-white max-sm:hidden" aria-label="Previous" onClick={() => go(-1)}><ChevronLeft className="size-6" /></Button>
          )}
          {after && (
            <Button variant="ghost" size="icon" className="absolute top-1/2 right-2 size-11 -translate-y-1/2 rounded-full bg-black/40 text-white hover:bg-black/60 hover:text-white max-sm:hidden" aria-label="Next" onClick={() => go(1)}><ChevronRight className="size-6" /></Button>
          )}
        </div>
        {info && (
          <aside aria-label="Details" className="shrink-0 overflow-y-auto bg-background text-foreground max-lg:absolute max-lg:inset-x-0 max-lg:bottom-0 max-lg:max-h-[75%] max-lg:rounded-t-2xl max-lg:shadow-2xl lg:w-96 lg:border-l">
            <Suspense fallback={<div className="p-4"><Skeleton className="h-64" /></div>}><InfoPanel id={asset.id} onClose={() => setInfo(false)} /></Suspense>
          </aside>
        )}
      </div>

      <AlbumPicker open={dialog === 'album'} ids={[asset.id]} onClose={() => setDialog(null)} />
      <Confirm
        open={dialog === 'delete'}
        title="Delete this file?"
        description="It moves to the trash and stays recoverable until the retention period ends. The original on your device is not touched."
        action="Delete"
        destructive
        onCancel={() => setDialog(null)}
        onConfirm={() => {
          setDialog(null)
          remove.mutate([{ device: asset.device, path: asset.path }], { onSuccess: moveOn })
        }}
      />
    </div>
  )
}
