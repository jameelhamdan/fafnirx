import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowDownUp, ChevronRight, Download, FolderPlus, Grid2x2, List, Search, Trash2, Upload, X } from 'lucide-react'
import { useEffect, useMemo, useRef, useState, type DragEvent, type FormEvent } from 'react'
import { useSearchParams } from 'react-router-dom'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Confirm } from '@/components/confirm'
import { DetailsSheet } from '@/components/files/details-sheet'
import { FileGrid } from '@/components/files/file-grid'
import { FileList } from '@/components/files/file-list'
import { entryKey } from '@/lib/entries'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup } from '@/components/files/toggle-group'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useNow } from '@/hooks/use-now'
import { api, downloadZip, query } from '@/lib/api'
import { count, joinPath } from '@/lib/format'
import type { DeviceSummary, Entry, Listing } from '@/lib/types'
import { collectDropped, enqueueUploads, fromInput } from '@/lib/upload'
import { cn } from '@/lib/utils'

const TYPES = [
  ['all', 'All files'],
  ['image', 'Images'],
  ['video', 'Videos'],
  ['audio', 'Audio'],
  ['document', 'Documents'],
  ['archive', 'Archives'],
] as const

const SORTS = [
  ['name', 'Name'],
  ['modified', 'Modified'],
  ['added', 'Recorded'],
  ['taken', 'Date taken'],
  ['size', 'Size'],
  ['type', 'Type'],
] as const

function NewFolder({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: (name: string) => void }) {
  const client = useQueryClient()
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post<{ device: DeviceSummary }>('/api/devices', { name, kind: 'local' }),
    onSuccess: (made) => {
      void client.invalidateQueries({ queryKey: ['devices'] })
      void client.invalidateQueries({ queryKey: ['files'] })
      setName('')
      onCreated(made.device.name)
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) { onClose(); create.reset() } }}>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>New local folder</DialogTitle>
            <DialogDescription>
              A top-level folder that lives on the master. Upload into it from this page, or copy files straight into <code>files/&lt;name&gt;/</code> on the server or its network share.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="folder-name">Name</Label>
            <Input id="folder-name" autoFocus placeholder="photos" value={name} onChange={(e) => setName(e.target.value)} />
            <p className="text-xs text-muted-foreground">Lowercase letters, digits, dot, underscore and dash.</p>
            {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          </div>
          <DialogFooter><Button type="submit" disabled={!name.trim() || create.isPending}>Create</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Files() {
  const [params, setParams] = useSearchParams()
  const client = useQueryClient()
  const now = useNow()
  const device = params.get('device') ?? ''
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
    const t = window.setTimeout(() => go({ q: search }, true), 300)
    return () => window.clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search])

  const [selected, setSelected] = useState<Set<string>>(new Set())
  useEffect(() => setSelected(new Set()), [device, path, q, type, showDeleted])
  const [newFolder, setNewFolder] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [dragging, setDragging] = useState(false)
  const fileInput = useRef<HTMLInputElement>(null)
  const folderInput = useRef<HTMLInputElement>(null)

  const devices = useQuery({ queryKey: ['devices'], queryFn: () => api.get<DeviceSummary[]>('/api/devices') })
  const canUpload = devices.data?.find((d) => d.name === device)?.kind === 'local'

  const listing = useInfiniteQuery({
    queryKey: ['files', device, path, q, type, sort, order, showDeleted],
    initialPageParam: 0,
    queryFn: ({ pageParam }) =>
      api.get<Listing>(`/api/files${query({ device, path, q, type: type === 'all' ? '' : type, sort, order, deleted: showDeleted, offset: pageParam })}`),
    getNextPageParam: (last, pages) => {
      if (last.device === null) return undefined
      const loaded = pages.reduce((n, p) => n + p.entries.filter((e) => e.kind === 'file').length, 0)
      return loaded < last.total ? loaded : undefined
    },
    placeholderData: (previous) => previous,
  })
  const entries = useMemo(() => listing.data?.pages.flatMap((p) => p.entries) ?? [], [listing.data])
  const total = listing.data?.pages[0]?.total ?? 0
  const searching = listing.data?.pages[0]?.searching ?? false

  const open = (e: Entry) => {
    if (e.kind === 'device') go({ device: e.name, path: null, q: null, type: null, file: null })
    else if (e.kind === 'dir') go({ path: e.path, q: null, file: null })
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

  const remove = useMutation({
    mutationFn: () => api.post<{ deleted: number }>('/api/files/delete', { items: chosen.map((e) => ({ device: e.device, path: e.path })) }),
    onSuccess: (r) => {
      toast.success(`${count(r.deleted)} file${r.deleted === 1 ? '' : 's'} deleted. Recoverable until the retention period ends.`)
      setSelected(new Set())
      void client.invalidateQueries({ queryKey: ['files'] })
    },
    onError: (e) => toast.error(e.message),
  })

  const upload = (files: { file: File; path: string }[]) => {
    if (files.length === 0) return
    enqueueUploads(device, files.map((f) => ({ file: f.file, path: joinPath(path, f.path) })))
  }
  const onDrop = async (e: DragEvent) => {
    e.preventDefault()
    setDragging(false)
    if (canUpload) upload(await collectDropped(e.dataTransfer))
  }

  const crumbs = [
    { name: 'Files', to: { device: null, path: null } },
    ...(device ? [{ name: device, to: { device, path: null } }] : []),
    ...path.split('/').filter(Boolean).map((part, i, all) => ({ name: part, to: { device, path: all.slice(0, i + 1).join('/') } })),
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
        <div className="pointer-events-none fixed inset-3 z-30 grid place-items-center rounded-xl border-2 border-dashed border-primary bg-primary/10 backdrop-blur-sm">
          <p className="text-lg font-medium">Drop to upload to {device}{path && `/${path}`}</p>
        </div>
      )}
      <PageHeader
        title="Files"
        actions={
          <>
            {!device && <Button variant="outline" onClick={() => setNewFolder(true)}><FolderPlus className="size-4" />New local folder</Button>}
            {device && (
              <Tooltip>
                <TooltipTrigger asChild>
                  <span className="flex gap-2">
                    <Button disabled={!canUpload} onClick={() => fileInput.current?.click()}><Upload className="size-4" />Upload</Button>
                    <Button variant="outline" disabled={!canUpload} onClick={() => folderInput.current?.click()}>Upload folder</Button>
                  </span>
                </TooltipTrigger>
                {!canUpload && <TooltipContent>Uploads go to local folders. Devices and sources are filled by their own sending.</TooltipContent>}
              </Tooltip>
            )}
          </>
        }
      />
      <input ref={fileInput} type="file" multiple hidden onChange={(e) => { if (e.target.files) upload(fromInput(e.target.files)); e.target.value = '' }} />
      <input ref={folderInput} type="file" hidden {...{ webkitdirectory: '' }} onChange={(e) => { if (e.target.files) upload(fromInput(e.target.files)); e.target.value = '' }} />

      <nav aria-label="Location" className="mb-3 flex flex-wrap items-center gap-1 text-sm">
        {crumbs.map((c, i) => (
          <span key={i} className="flex items-center gap-1">
            {i > 0 && <ChevronRight className="size-3.5 text-muted-foreground" />}
            {i === crumbs.length - 1 ? <span className="font-medium">{c.name}</span> : (
              <button type="button" className="text-muted-foreground hover:text-foreground hover:underline" onClick={() => go({ ...c.to, q: null, file: null })}>{c.name}</button>
            )}
          </span>
        ))}
      </nav>

      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative min-w-48 flex-1 sm:max-w-xs">
          <Search className="absolute top-2.5 left-2.5 size-4 text-muted-foreground" />
          <Input value={search} onChange={(e) => setSearch(e.target.value)} placeholder={device ? `Search in ${device}${path ? `/${path}` : ''}` : 'Search a device first'} disabled={!device} className="pl-8" aria-label="Search" />
        </div>
        <Select value={type} onValueChange={(v) => go({ type: v === 'all' ? null : v }, true)} disabled={!device}>
          <SelectTrigger className="w-36" aria-label="File type"><SelectValue /></SelectTrigger>
          <SelectContent>{TYPES.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
        </Select>
        <Select value={sort} onValueChange={(v) => go({ sort: v === 'name' ? null : v }, true)} disabled={!device}>
          <SelectTrigger className="w-36" aria-label="Sort by"><SelectValue /></SelectTrigger>
          <SelectContent>{SORTS.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
        </Select>
        <Button variant="outline" size="icon" disabled={!device} aria-label={order === 'asc' ? 'Ascending' : 'Descending'} onClick={() => go({ order: order === 'asc' ? 'desc' : null }, true)}>
          <ArrowDownUp className={cn('size-4', order === 'desc' && 'rotate-180')} />
        </Button>
        <label className="flex items-center gap-2 px-1 text-sm text-muted-foreground">
          <Switch checked={showDeleted} onCheckedChange={(v) => go({ deleted: v ? '1' : null }, true)} disabled={!device} />
          Deleted
        </label>
        <div className="ml-auto">
          <ToggleGroup
            value={view}
            onChange={(v) => go({ view: v === 'grid' ? 'grid' : null }, true)}
            options={[{ value: 'list', label: 'List', icon: List }, { value: 'grid', label: 'Grid', icon: Grid2x2 }]}
          />
        </div>
      </div>

      {chosen.length > 0 && (
        <div className="sticky top-16 z-10 mb-3 flex flex-wrap items-center gap-2 rounded-lg border bg-popover p-2 shadow-sm">
          <Button variant="ghost" size="icon" className="size-8" aria-label="Clear selection" onClick={() => setSelected(new Set())}><X className="size-4" /></Button>
          <span className="text-sm font-medium">{chosen.length} selected</span>
          <div className="ml-auto flex gap-2">
            <Button size="sm" variant="outline" onClick={() => downloadZip(chosen.map((e) => ({ device: e.device, path: e.kind === 'device' ? '' : e.path })), chosen.length === 1 ? chosen[0].name : device || 'fafnirx')}>
              <Download className="size-4" />Download{chosen.length === 1 && chosen[0].kind === 'file' ? '' : ' as zip'}
            </Button>
            <Button size="sm" variant="outline" disabled={chosen.some((e) => e.kind === 'device')} onClick={() => setConfirmDelete(true)}><Trash2 className="size-4" />Delete</Button>
          </div>
        </div>
      )}

      {listing.error ? <ErrorNote error={listing.error} /> : !listing.data ? (
        <Skeleton className="h-96" />
      ) : entries.length === 0 ? (
        !device ? (
          <Empty title="No folders yet">Add a device on the Devices page, or create a local folder to upload into.</Empty>
        ) : filtering ? (
          <Empty title="Nothing matches">Try a different search or file type.</Empty>
        ) : (
          <Empty title="This folder is empty">{canUpload ? 'Drop files here or use Upload.' : showDeleted ? '' : 'Nothing has been collected here yet.'}</Empty>
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
              {searching ? `${count(total)} match${total === 1 ? '' : 'es'}` : device ? `${count(entries.length)} item${entries.length === 1 ? '' : 's'}${total > entries.filter((e) => e.kind === 'file').length ? ` of ${count(total)} files` : ''}` : ''}
            </span>
            {listing.hasNextPage && (
              <Button variant="outline" size="sm" disabled={listing.isFetchingNextPage} onClick={() => void listing.fetchNextPage()}>
                {listing.isFetchingNextPage ? 'Loading…' : 'Load more'}
              </Button>
            )}
          </div>
        </>
      )}

      <DetailsSheet device={device} path={openFile} onClose={() => go({ file: null }, true)} onDeleted={() => { go({ file: null }, true); void client.invalidateQueries({ queryKey: ['files'] }) }} />
      <NewFolder open={newFolder} onClose={() => setNewFolder(false)} onCreated={(name) => { setNewFolder(false); go({ device: name, path: null }) }} />
      <Confirm
        open={confirmDelete}
        title={`Delete ${chosen.length} item${chosen.length === 1 ? '' : 's'}?`}
        description="Folders delete everything inside. Deleted files stay recoverable until the retention period ends."
        action="Delete"
        destructive
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => { setConfirmDelete(false); remove.mutate() }}
      />
    </div>
  )
}
