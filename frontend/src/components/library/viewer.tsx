import { Archive, ArchiveRestore, ChevronLeft, ChevronRight, Download, FileImage, FolderPlus, Heart, Info, Link2, Loader2, MoreVertical, Trash2, X } from 'lucide-react'
import { lazy, Suspense, useEffect, useRef, useState, type PointerEvent } from 'react'
import { toast } from 'sonner'
import { usePicture, useReadyUrl } from '@/lib/pictures'
import { Button } from '../ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '../ui/dropdown-menu'
import { Skeleton } from '../ui/skeleton'
import { Confirm } from '../confirm'
import { NativeVideo } from '../native-video'
import { mediaUrl } from '@/lib/api'
import { cn } from '@/lib/utils'
import { whenText } from '@/lib/format'
import { useLibraryActions } from '@/lib/library'
import { fileOfAsset } from '@/lib/module-file'
import { hasWorkspace, useRole } from '@/lib/session'
import type { Asset } from '@/lib/types'
import { AlbumPicker } from './album-picker'
import { ModuleViewer, useHasViewer } from '@/sdk'
import { useT } from '@/lib/i18n'

const InfoPanel = lazy(() => import('./info-panel'))

/** Round buttons over the picture: frosted, so that they read on a bright photo and a dark one. */
const glass = 'rounded-full border border-white/10 bg-black/35 text-white backdrop-blur-md hover:bg-black/55 hover:text-white'
const bare = 'text-white hover:bg-white/15 hover:text-white'

/** True when the key press belongs to something else (a field, a menu, a dialog on top). */
function busyElsewhere(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null
  return !!target?.closest('input, textarea, select, [contenteditable="true"]') || !!document.querySelector('[data-slot="dialog-content"], [role="menu"]')
}

/** `video`: the file is a video. */
function Unavailable({ asset, video, retry }: { asset: Asset; video?: boolean; retry?: () => void }) {
  const t = useT()
  return (
    <div className="flex max-w-sm flex-col items-center gap-3 p-6 text-center text-white/90">
      <FileImage className="size-10 text-white/60" />
      <p className="text-sm">
        {video ? t('This video cannot be played in the browser. Download it to watch it.') : t('No picture could be made of this file.')}
      </p>
      <div className="flex gap-2">
        {retry && <Button variant="secondary" size="sm" onClick={retry}>{t('Try again')}</Button>}
        <Button variant="secondary" size="sm" asChild><a href={mediaUrl(asset.id, 'download', asset.hash)}><Download className="size-4" />{t('Download the original')}</a></Button>
      </div>
    </div>
  )
}

/** A photo that zooms in on a double click (or a tap on a touch screen twice) and follows the pointer while zoomed. */
function Photo({ asset, onFail }: { asset: Asset; onFail: () => void }) {
  const t = useT()
  const [zoom, setZoom] = useState<{ x: number; y: number } | null>(null)
  // The preview is made when it is first looked at: a spinner until it is there.
  const picture = usePicture(mediaUrl(asset.id, 'preview', asset.hash))
  useEffect(() => {
    if (picture.state === 'none') onFail()
  }, [picture.state, onFail])
  const at = (e: PointerEvent<HTMLElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    return { x: ((e.clientX - rect.left) / rect.width) * 100, y: ((e.clientY - rect.top) / rect.height) * 100 }
  }
  const waiting = picture.state === 'pending'
  return (
    <>
      {waiting && <Loader2 className="size-8 animate-spin text-white/70" aria-label={t('Making the picture')} />}
    <img
      key={picture.attempt}
      src={mediaUrl(asset.id, 'preview', asset.hash)}
      alt={asset.name}
      draggable={false}
      title={zoom ? undefined : t('Double-click to zoom')}
      onLoad={picture.onLoad}
      onError={picture.onError}
      onDoubleClick={(e) => setZoom(zoom ? null : at(e as unknown as PointerEvent<HTMLElement>))}
      onPointerMove={(e) => { if (zoom) setZoom(at(e)) }}
      style={zoom ? { transform: 'scale(2.5)', transformOrigin: `${zoom.x}% ${zoom.y}%` } : undefined}
      className={cn(
        'relative max-h-full max-w-full object-contain shadow-2xl select-none sm:rounded-sm',
        waiting && 'hidden',
        'transition-transform duration-200 ease-out',
        zoom ? 'cursor-zoom-out' : 'cursor-zoom-in',
      )}
    />
    </>
  )
}

/** The thumbnail of the file, blurred, behind the picture. It is not waited for: it shows when it is there. */
function Backdrop({ src }: { src: string }) {
  const picture = usePicture(src)
  if (picture.state === 'none') return null
  return (
    <img
      key={picture.attempt}
      src={src}
      alt=""
      aria-hidden
      draggable={false}
      onLoad={picture.onLoad}
      onError={picture.onError}
      className="pointer-events-none absolute inset-0 size-full scale-125 object-cover opacity-40 blur-3xl saturate-150"
    />
  )
}

function Media({ asset }: { asset: Asset }) {
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)
  const made = fileOfAsset(asset)
  // The poster of a video is made when it is first looked at: it is given to the player once it is there.
  const poster = useReadyUrl(made.poster)
  const file = { ...made, poster }
  // A module that is up shows what it can (videos, with the video module); otherwise a photo is shown here.
  const hasViewer = useHasViewer(file)
  const retry = () => { setFailed(false); setAttempt((n) => n + 1) }
  if (failed) return <Unavailable asset={asset} video={asset.kind === 'video'} retry={retry} />
  if (hasViewer) {
    return (
      <ModuleViewer
        key={attempt}
        file={file}
        onFail={() => setFailed(true)}
        globalKeys
        loading={<Skeleton className="h-64 w-96 max-w-full bg-white/10" />}
      />
    )
  }
  // Without the video module the original plays as it is, in the browser's own player.
  if (asset.kind === 'video') return <NativeVideo key={attempt} src={mediaUrl(asset.id, 'original', asset.hash)} poster={poster} onFail={() => setFailed(true)} />
  return <Photo key={attempt} asset={asset} onFail={() => setFailed(true)} />
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
  id: string
  onChange: (id: string) => void
  onClose: () => void
  /** Called when the viewer gets near the end of what it was given, so that the page can load more. */
  onEdge?: (direction: 'prev' | 'next') => void
}) {
  const t = useT()
  // A read-only user looks at files, downloads and streams them; everything else is for the other roles.
  const mine = hasWorkspace(useRole())
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
  /** Puts the address of this file on its own on the clipboard, to send to whoever may see it (shown when the page may not copy). */
  const copyLink = () => {
    if (!asset) return
    const link = `${window.location.origin}/watch/${asset.id}`
    if (navigator.clipboard) navigator.clipboard.writeText(link).then(() => toast.success(t('Link copied')), () => toast(link))
    else toast(link)
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
      <header className="z-10 flex items-center gap-1 bg-gradient-to-b from-black/75 via-black/30 to-transparent p-2 pb-6">
        <Button variant="ghost" size="icon" className={bare} aria-label={t('Close')} onClick={onClose}><X className="size-5" /></Button>
        <div className="mr-auto min-w-0 px-2">
          <p className="truncate text-sm font-medium">{asset.name}</p>
          <p className="text-xs text-white/70">{whenText(asset.when)}{asset.processing && <span className="ml-2">· {t('Still being processed')}</span>}</p>
        </div>
        {mine && (
          <Button variant="ghost" size="icon" className={bare} aria-label={asset.favorite ? t('Remove from favorites') : t('Add to favorites')} aria-pressed={asset.favorite} onClick={() => edit.mutate({ id: asset.id, favorite: !asset.favorite })}>
            <Heart className={asset.favorite ? 'size-5 fill-current' : 'size-5'} />
          </Button>
        )}
        <Button variant="ghost" size="icon" className={bare} aria-label={t('Info')} aria-pressed={info} onClick={() => setInfo((v) => !v)}><Info className="size-5" /></Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className={bare} aria-label={t('More actions')}><MoreVertical className="size-5" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={copyLink}><Link2 className="size-4" />{t('Copy link')}</DropdownMenuItem>
            {mine && <DropdownMenuItem onSelect={() => setDialog('album')}><FolderPlus className="size-4" />{t('Add to album')}</DropdownMenuItem>}
            <DropdownMenuItem asChild><a href={mediaUrl(asset.id, 'download', asset.hash)}><Download className="size-4" />{t('Download')}</a></DropdownMenuItem>
            {mine && (
              <DropdownMenuItem onSelect={() => edit.mutate({ id: asset.id, archived: !asset.archived }, { onSuccess: moveOn })}>
                {asset.archived ? <ArchiveRestore className="size-4" /> : <Archive className="size-4" />}{asset.archived ? t('Unarchive') : t('Archive')}
              </DropdownMenuItem>
            )}
            {mine && <DropdownMenuSeparator />}
            {mine && <DropdownMenuItem variant="destructive" onSelect={() => setDialog('delete')}><Trash2 className="size-4" />{t('Delete')}</DropdownMenuItem>}
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      <div className="relative flex min-h-0 flex-1">
        <div
          className="relative grid min-w-0 flex-1 place-items-center overflow-hidden bg-neutral-950"
          onTouchStart={(e) => { touch.current = e.touches.length === 1 ? e.touches[0].clientX : null }}
          onTouchEnd={(e) => {
            const start = touch.current
            touch.current = null
            if (start === null) return
            const dx = e.changedTouches[0].clientX - start
            if (Math.abs(dx) > 70) go(dx > 0 ? -1 : 1)
          }}
        >
          {/* The picture itself, blurred and dimmed, fills what the picture leaves empty. */}
          <Backdrop key={asset.id} src={mediaUrl(asset.id, 'thumb', asset.hash)} />
          <div key={asset.id} className="animate-in fade-in absolute inset-0 z-[1] flex items-center justify-center duration-200">
            <Media asset={asset} />
          </div>
          {before && (
            <Button variant="ghost" size="icon" className={cn(glass, 'absolute top-1/2 left-2 z-[2] size-11 -translate-y-1/2 max-sm:hidden')} aria-label={t('Previous')} onClick={() => go(-1)}><ChevronLeft className="size-6" /></Button>
          )}
          {after && (
            <Button variant="ghost" size="icon" className={cn(glass, 'absolute top-1/2 right-2 z-[2] size-11 -translate-y-1/2 max-sm:hidden')} aria-label={t('Next')} onClick={() => go(1)}><ChevronRight className="size-6" /></Button>
          )}
        </div>
        {info && (
          <aside aria-label={t('Details')} className="shrink-0 overflow-y-auto bg-background text-foreground max-lg:absolute max-lg:inset-x-0 max-lg:bottom-0 max-lg:max-h-[75%] max-lg:rounded-t-2xl max-lg:shadow-2xl lg:w-96 lg:border-l">
            <Suspense fallback={<div className="p-4"><Skeleton className="h-64" /></div>}><InfoPanel id={asset.id} onClose={() => setInfo(false)} /></Suspense>
          </aside>
        )}
      </div>

      <AlbumPicker open={dialog === 'album'} ids={[asset.id]} onClose={() => setDialog(null)} />
      <Confirm
        open={dialog === 'delete'}
        title={t('Delete this file?')}
        description={t('It moves to the trash and stays recoverable until the retention period ends. The original on your device is not touched.')}
        action={t('Delete')}
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
