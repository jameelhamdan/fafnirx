import { useInfiniteQuery, useQueryClient } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { ArrowDownUp, ChevronRight, Download, FolderInput, FolderPlus, Grid2x2, Images, List, Pencil, Search, Trash2, Upload, X } from 'lucide-react'
import { useEffect, useMemo, useRef, useState, type DragEvent } from 'react'
import { useSearchParams } from 'react-router-dom'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Confirm } from '@/components/confirm'
import { DetailsSheet } from '@/components/files/details-sheet'
import { FileGrid } from '@/components/files/file-grid'
import { FileList } from '@/components/files/file-list'
import { MoveDialog } from '@/components/files/move-dialog'
import { NewFolder } from '@/components/new-folder'
import { FolderAlbum, FolderAlbumButton, type FolderRef } from '@/components/files/folder-album'
import { RenameFolder } from '@/components/files/rename-folder'
import { entryKey } from '@/lib/entries'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useNow } from '@/hooks/use-now'
import { api, downloadZip, query } from '@/lib/api'
import { count, joinPath, plural } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import { isLibraryFile } from '@/lib/entries'
import { hasWorkspace, useRole } from '@/lib/session'
import { useOpenMedia } from '@/lib/viewer-source'
import { AccessBadge } from '@/components/files/access-badge'
import { RescanButton } from '@/components/files/rescan-button'
import type { Entry, Listing } from '@/lib/types'
import { collectDropped, enqueueUploads, fromInput } from '@/lib/upload'
import { cn } from '@/lib/utils'

const TYPES = [
  ['all', msg('All files')],
  ['image', msg('Images')],
  ['video', msg('Videos')],
  ['audio', msg('Audio')],
  ['document', msg('Documents')],
  ['archive', msg('Archives')],
] as const

const SORTS = [
  ['name', msg('Name')],
  ['modified', msg('Modified')],
  ['added', msg('Recorded')],
  ['taken', msg('Date taken')],
  ['size', msg('Size')],
  ['type', msg('Type')],
] as const

/** The last part of a path or `user/folder`: what a download is called (a slash cannot be in a file name). */
const lastPart = (name: string) => name.split('/').filter(Boolean).pop() ?? ''

export default function Files() {
  const t = useT()
  const [params, setParams] = useSearchParams()
  const client = useQueryClient()
  const now = useNow()
  const folder = params.get('folder') ?? ''
  const path = params.get('path') ?? ''
  const q = params.get('q') ?? ''
  const type = params.get('type') ?? 'all'
  const sort = params.get('sort') ?? 'name'
  const order = params.get('order') ?? 'asc'
  const view = params.get('view') === 'grid' ? 'grid' : 'list'
  const showDeleted = params.get('deleted') === '1'
  const openFile = params.get('file')

  /** Changes some of the address' parameters; `null` removes one. */
  const go = (changes: Record<string, string | null>, replace = false) => {
    const next = new URLSearchParams(params)
    for (const [k, v] of Object.entries(changes)) {
      if (v === null || v === '') next.delete(k)
      else next.set(k, v)
    }
    setParams(next, { replace })
  }

  const [search, setSearch] = useState(q)
  useEffect(() => setSearch(q), [q])
  useEffect(() => {
    if (search === q) return
    const timer = window.setTimeout(() => go({ q: search }, true), 300)
    return () => window.clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search])

  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [moving, setMoving] = useState<Entry | null>(null)
  useEffect(() => setSelected(new Set()), [folder, path, q, type, showDeleted])
  const [newFolder, setNewFolder] = useState(false)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [dragging, setDragging] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)
  const folderInput = useRef<HTMLInputElement>(null)

  const role = useRole()

  const listing = useInfiniteQuery({
    queryKey: ['files', folder, path, q, type, sort, order, showDeleted],
    initialPageParam: 0,
    queryFn: ({ pageParam }) =>
      api.get<Listing>(`/api/files${query({ folder, path, q, type: type === 'all' ? '' : type, sort, order, deleted: showDeleted, offset: pageParam })}`),
    getNextPageParam: (last, pages) => {
      if (last.folder === null) return undefined
      const loaded = pages.reduce((n, p) => n + p.entries.filter((e) => e.kind === 'file').length, 0)
      return loaded < last.total ? loaded : undefined
    },
    placeholderData: (previous) => previous,
  })
  const entries = useMemo(() => listing.data?.pages.flatMap((p) => p.entries) ?? [], [listing.data])
  const total = listing.data?.pages[0]?.total ?? 0
  const searching = listing.data?.pages[0]?.searching ?? false
  // The master says what the web app may do here: the master files, local folders and shared folders can be changed, what
  // a folder or a source sends cannot.
  const canUpload = listing.data?.pages[0]?.writable ?? false

  const openMedia = useOpenMedia()
  const open = (e: Entry) => {
    if (e.kind === 'folder') go({ folder: e.folder, path: null, q: null, type: null, file: null })
    else if (e.kind === 'dir') go({ path: e.path, q: null, file: null })
    // A photo or video is shown in the viewer, at an address of its own that can be sent on; the rest in the details.
    else if (e.id && e.deleted_at === null && isLibraryFile(e.name)) openMedia(e.id)
    else go({ file: e.path })
  }
  const toggle = (key: string) =>
    setSelected((s) => {
      const n = new Set(s)
      if (!n.delete(key)) n.add(key)
      return n
    })
  const toggleAll = () => setSelected((s) => (entries.every((e) => s.has(entryKey(e))) ? new Set() : new Set(entries.map(entryKey))))
  const chosen = entries.filter((e) => selected.has(entryKey(e)))

  // A folder can be shown as an album (a show, a trip): the dialog makes it, renames it or ends it.
  const [albumOf, setAlbumOf] = useState<FolderRef | null>(null)
  const remove = useApiMutation(() => api.post<{ deleted: number }>('/api/files/delete', { items: chosen.map((e) => ({ folder: e.folder, path: e.path })) }), {
    onSuccess: (r) => {
      toast.success(t('{files} deleted. Recoverable until the retention period ends.', { files: plural(r.deleted, 'file') }))
      setSelected(new Set())
    },
    invalidate: [['files']],
  })

  const upload = (files: { file: File; path: string }[]) => {
    if (files.length === 0) return
    enqueueUploads(folder, files.map((f) => ({ file: f.file, path: joinPath(path, f.path) })))
  }
  const onDrop = async (e: DragEvent) => {
    e.preventDefault()
    setDragging(false)
    if (canUpload) upload(await collectDropped(e.dataTransfer))
  }

  const crumbs = [
    { name: t('Files'), to: { folder: null, path: null } },
    ...(folder ? [{ name: folder, to: { folder, path: null } }] : []),
    ...path.split('/').filter(Boolean).map((part, i, all) => ({ name: part, to: { folder, path: all.slice(0, i + 1).join('/') } })),
  ]
  const filtering = q !== '' || type !== 'all'

  return (
    <div
      onDragOver={(e) => { if (canUpload) { e.preventDefault(); setDragging(true) } }}
      onDragLeave={(e) => { if (e.currentTarget === e.target) setDragging(false) }}
      onDrop={onDrop}
      className="relative"
    >
      {dragging && (
        <div className="pointer-events-none fixed inset-3 z-30 grid place-items-center rounded-md border-2 border-dashed border-primary bg-primary/10 backdrop-blur-sm">
          <p className="text-lg font-medium">{t('Drop to upload to {target}', { target: folder + (path ? `/${path}` : '') })}</p>
        </div>
      )}
      <PageHeader
        title={t('Files')}
        actions={
          <>
            {role === 'admin' && <RescanButton />}
            {!folder && hasWorkspace(role) && <Button variant="outline" onClick={() => setNewFolder(true)}><FolderPlus className="size-4" />{t('New folder')}</Button>}
            {folder && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="flex gap-2">
                    <Button disabled={!canUpload} onClick={() => fileInput.current?.click()}><Upload className="size-4" />{t('Upload')}</Button>
                    <Button variant="outline" disabled={!canUpload} onClick={() => folderInput.current?.click()}>{t('Upload folder')}</Button>
                  </span>
                </TooltipTrigger>
                {!canUpload && <TooltipContent>{t('Uploads go to your folders and the master files. A device’s files come from the device itself.')}</TooltipContent>}
              </Tooltip>
            )}
          </>
        }
      />
      <input ref={fileInput} type="file" multiple hidden onChange={(e) => { if (e.target.files) upload(fromInput(e.target.files)); e.target.value = '' }} />
      <input ref={folderInput} type="file" hidden {...{ webkitdirectory: '' }} onChange={(e) => { if (e.target.files) upload(fromInput(e.target.files)); e.target.value = '' }} />

      {/* At the top the page title says where you are; the trail shows from the first folder down. */}
      {folder && <nav aria-label={t('Location')} className="mb-3 flex flex-wrap items-center gap-1 text-sm">
        {crumbs.map((c, i) => (
          <span key={i} className="flex items-center gap-1">
            {i > 0 && <ChevronRight className="size-3.5 text-muted-foreground" />}
            {i === crumbs.length - 1 ? <span className="font-medium">{c.name}</span> : (
              <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => go({ ...c.to, q: null, file: null })}>{c.name}</button>
            )}
          </span>
        ))}
        {folder && listing.data && (
          <span className="ms-2"><AccessBadge writable={canUpload} kind={listing.data.pages[0]?.folder_kind ?? null} /></span>
        )}
      </nav>}

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative min-w-48 flex-1 sm:max-w-xs">
          <Search className="absolute top-2.5 start-2.5 size-4 text-muted-foreground" />
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={folder ? t('Search in {where}', { where: folder + (path ? `/${path}` : '') }) : t('Search a folder first')} disabled={!folder} className="ps-8" aria-label={t('Search')} />
        </div>
        <Select value={type} onValueChange={(v) => go({ type: v === 'all' ? null : v }, true)} disabled={!folder}>
          <SelectTrigger className="w-36" aria-label={t('File type')}><SelectValue /></SelectTrigger>
          <SelectContent>{TYPES.map(([v, l]) => <SelectItem key={v} value={v}>{t(l)}</SelectItem>)}</SelectContent>
        </Select>
        <Select value={sort} onValueChange={(v) => go({ sort: v === 'name' ? null : v }, true)} disabled={!folder}>
          <SelectTrigger className="w-36" aria-label={t('Sort by')}><SelectValue /></SelectTrigger>
          <SelectContent>{SORTS.map(([v, l]) => <SelectItem key={v} value={v}>{t(l)}</SelectItem>)}</SelectContent>
        </Select>
        <Button variant="outline" size="icon" disabled={!folder} aria-label={order === 'asc' ? t('Ascending') : t('Descending')} onClick={() => go({ order: order === 'asc' ? 'desc' : null }, true)}>
          <ArrowDownUp className={cn('size-4', order === 'desc' && 'rotate-180')} />
        </Button>
        <label className="flex items-center gap-2 px-1 text-sm text-muted-foreground">
          <Switch checked={showDeleted} onCheckedChange={(v) => go({ deleted: v ? '1' : null }, true)} disabled={!folder} />
          {t('Deleted')}
        </label>
        <div className="ms-auto flex items-center gap-2">
          {folder && !path && hasWorkspace(role) && !showDeleted && <FolderAlbumButton folder={{ folder, path }} />}
          <ToggleGroup value={view} onValueChange={(v) => v && go({ view: v === 'grid' ? 'grid' : null }, true)}>
            <ToggleGroupItem value="list" aria-label={t('List')} title={t('List')}><List /></ToggleGroupItem>
            <ToggleGroupItem value="grid" aria-label={t('Grid')} title={t('Grid')}><Grid2x2 /></ToggleGroupItem>
          </ToggleGroup>
        </div>
      </div>

      {chosen.length > 0 && (
        <div className="sticky top-16 z-10 mb-3 flex flex-wrap items-center gap-2 rounded-md border bg-popover p-2 ">
          <Button variant="ghost" size="icon" className="size-8" aria-label={t('Clear selection')} onClick={() => setSelected(new Set())}><X className="size-4" /></Button>
          <span className="text-sm font-medium">{t('{count} selected', { count: chosen.length })}</span>
          <div className="ms-auto flex gap-2">
            <Button size="sm" variant="outline" onClick={() => downloadZip(chosen.map((e) => ({ folder: e.folder, path: e.kind === 'folder' ? '' : e.path })), lastPart(chosen.length === 1 ? chosen[0].name : folder) || 'fafnirx')}>
              <Download className="size-4" />{chosen.length === 1 && chosen[0].kind === 'file' ? t('Download') : t('Download as zip')}
            </Button>
            {hasWorkspace(role) && !showDeleted && chosen.length === 1 && chosen[0].kind === 'folder' && chosen[0].writable && chosen[0].folder_kind === 'user' && (
              <Button size="sm" variant="outline" onClick={() => setRenaming(chosen[0].folder)}><Pencil className="size-4" />{t('Rename folder')}</Button>
            )}
            {hasWorkspace(role) && !showDeleted && chosen.length === 1 && chosen[0].kind === 'folder' && ['user', 'master'].includes(chosen[0].folder_kind ?? '') && (
              <Button size="sm" variant="outline" onClick={() => setAlbumOf({ folder: chosen[0].folder, path: '' })}>
                <Images className="size-4" />{t('Show as album')}
              </Button>
            )}
            {hasWorkspace(role) && canUpload && !showDeleted && chosen.length === 1 && chosen[0].kind !== 'folder' && (
              <Button size="sm" variant="outline" onClick={() => setMoving(chosen[0])}><FolderInput className="size-4" />{t('Move or rename')}</Button>
            )}
            <Button size="sm" variant="outline" disabled={chosen.some((e) => e.kind === 'folder')} onClick={() => setConfirmDelete(true)}><Trash2 className="size-4" />{t('Delete')}</Button>
          </div>
        </div>
      )}

      {listing.error ? <ErrorNote error={listing.error} /> : !listing.data ? (
        <Skeleton className="h-96" />
      ) : entries.length === 0 ? (
        !folder ? (
          <Empty title={t('No folders yet')}>{t('Add a device or a folder on the Devices page.')}</Empty>
        ) : filtering ? (
          <Empty title={t('Nothing matches')}>{t('Try a different search or file type.')}</Empty>
        ) : (
          <Empty title={t('This folder is empty')}>{canUpload ? t('Drop files here or use Upload.') : showDeleted ? '' : t('Nothing has been collected here yet.')}</Empty>
        )
      ) : (
        <>
          {view === 'list' ? (
            <FileList entries={entries} selected={selected} onToggle={toggle} onToggleAll={toggleAll} onOpen={open} sort={sort} order={order} now={now}
              onSort={(f) => go(f === sort ? { order: order === 'asc' ? 'desc' : null } : { sort: f === 'name' ? null : f, order: null }, true)} />
          ) : (
            <FileGrid entries={entries} selected={selected} onToggle={toggle} onOpen={open} />
          )}
          <div className="mt-3 flex items-center justify-between text-sm text-muted-foreground">
            <span>
              {searching ? t('{count} matches', { count: count(total) }) : folder ? plural(entries.length, 'item') + (total > entries.filter((e) => e.kind === 'file').length ? ' ' + t('of {total} files', { total: count(total) }) : '') : ''}
            </span>
            {listing.hasNextPage && (
              <Button variant="outline" size="sm" disabled={listing.isFetchingNextPage} onClick={() => void listing.fetchNextPage()}>
                {listing.isFetchingNextPage ? t('Loading…') : t('Load more')}
              </Button>
            )}
          </div>
        </>
      )}

      <DetailsSheet folder={folder} path={openFile} onClose={() => go({ file: null }, true)} onDeleted={() => { go({ file: null }, true); void client.invalidateQueries({ queryKey: ['files'] }) }} />
      {moving && <MoveDialog folder={moving.folder} path={moving.path} onClose={() => setMoving(null)} onMoved={() => { setMoving(null); setSelected(new Set()) }} />}
      <FolderAlbum folder={albumOf} onClose={() => setAlbumOf(null)} />
      <RenameFolder folder={renaming} onClose={() => setRenaming(null)} onRenamed={(folder) => { setRenaming(null); setSelected(new Set()); if (folder) go({ folder: folder, path: null }) }} />
      <NewFolder open={newFolder} onClose={() => setNewFolder(false)} onCreated={(folder) => { setNewFolder(false); go({ folder: folder, path: null }) }} />
      <Confirm
        open={confirmDelete}
        title={t('Delete {items}?', { items: plural(chosen.length, 'item') })}
        description={t('Folders delete everything inside. Deleted files stay recoverable until the retention period ends.')}
        action={t('Delete')}
        destructive
        onClose={() => setConfirmDelete(false)}
        onConfirm={() => remove.mutate()}
      />
    </div>
  )
}
