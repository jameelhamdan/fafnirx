import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Copy, FolderOpen, FolderSync, LogOut, MoreHorizontal, Plus, Trash2, UserPlus, X } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { Confirm } from '@/components/confirm'
import { SharedStateList } from '@/components/shared-state'
import { Empty, ErrorNote, PageHeader, SELECT } from '@/components/common'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, bytes, count } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'
import { cn } from '@/lib/utils'
import type { SharedFolder } from '@/lib/types'

type Show = 'all' | 'mine' | 'others'
type Person = { name: string; can_write: boolean }

/** Refetches the shared folders after a change. */
function useRefreshShared() {
  const client = useQueryClient()
  return () => void client.invalidateQueries({ queryKey: ['shared'] })
}

function AccessSelect({ canWrite, onChange, label }: { canWrite: boolean; onChange: (canWrite: boolean) => void; label: string }) {
  const t = useT()
  return (
    <select className={SELECT} aria-label={label} value={canWrite ? 'write' : 'read'} onChange={(e) => onChange(e.target.value === 'write')}>
      <option value="write">{t('Read and change')}</option>
      <option value="read">{t('Read only')}</option>
    </select>
  )
}

/** A round mark with the first letter of a user's name. */
function Avatar({ name, className }: { name: string; className?: string }) {
  return (
    <span aria-hidden className={cn('inline-flex size-7 shrink-0 items-center justify-center rounded-full bg-secondary text-xs font-medium uppercase ring-2 ring-background', className)}>
      {name.slice(0, 1)}
    </span>
  )
}

/** How the computers that have the folder are doing, in one word. */
function syncBadge(folder: SharedFolder, t: ReturnType<typeof useT>) {
  const rows = folder.computers
  if (rows.length === 0) return <Badge variant="outline">{t('Not attached')}</Badge>
  if (rows.some((r) => r.last_error !== '' || r.failing > 0)) return <Badge variant="destructive">{t('Needs attention')}</Badge>
  if (rows.some((r) => r.last_cycle === 0)) return <Badge variant="outline">{t('Not synced yet')}</Badge>
  return <Badge variant="secondary">{t('In step')}</Badge>
}

function AddShared({ onMade }: { onMade: (folder: string) => void }) {
  const t = useT()
  const me = useSession()?.user?.name
  const refresh = useRefreshShared()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post('/api/shared', { name }),
    onSuccess: () => {
      const made = `${me ?? ''}/${name.trim().toLowerCase()}`
      setOpen(false)
      setName('')
      refresh()
      onMade(made) // the next step, sharing it and attaching it to a computer, is in the panel that opens
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />{t('New shared folder')}</Button></DialogTrigger>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('New shared folder')}</DialogTitle>
            <DialogDescription>{t('A folder of yours that all your computers can keep in step. It is empty until a computer attaches a folder to it.')}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="shared-name">{t('Name')}</Label>
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">{me}/</span>
              <Input id="shared-name" autoFocus placeholder="family" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <p className="text-xs text-muted-foreground">{t('Lowercase letters, digits, dot, underscore and dash; up to 63 characters.')}</p>
            {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          </div>
          <DialogFooter><Button type="submit" disabled={!name.trim() || create.isPending}>{t('Create')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** The people a folder is shared with, and the way to add more: the part of the panel only its owner sees. */
function People({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const me = useSession()?.user?.name ?? ''
  const refresh = useRefreshShared()
  const [names, setNames] = useState('')
  const [canWrite, setCanWrite] = useState(true)
  const members = `/api/shared/${encodeURIComponent(folder.name)}/members`
  const people = useQuery({ queryKey: ['shared', 'people'], queryFn: () => api.get<Person[]>('/api/shared/people'), staleTime: 60_000 })
  const suggestions = (people.data ?? []).filter((p) => !folder.members.some((m) => m.user === p.name))
  // Adding a user and changing what they may do are the same request.
  const save = useMutation({ mutationFn: (m: { user: string; can_write: boolean }) => api.post(members, m), onSuccess: refresh, onError: (e) => toast.error(e.message) })
  const remove = useMutation({
    mutationFn: (m: { user: string; can_write: boolean }) => api.delete(`${members}/${encodeURIComponent(m.user)}`),
    onSuccess: (_, m) => {
      refresh()
      toast(t('Stopped sharing with {name}', { name: m.user }), { action: { label: t('Undo'), onClick: () => save.mutate(m) } })
    },
    onError: (e) => toast.error(e.message),
  })
  // Several names at once, separated by commas or spaces; the ones that fail stay in the field with the reason.
  const add = useMutation({
    mutationFn: async () => {
      const failed: { name: string; why: string }[] = []
      for (const name of names.split(/[\s,;]+/).filter(Boolean)) {
        try {
          // A read-only account can only read: give it that instead of failing.
          const reader = people.data?.find((p) => p.name === name.toLowerCase())?.can_write === false
          await api.post(members, { user: name, can_write: canWrite && !reader })
        } catch (e) {
          failed.push({ name, why: e instanceof Error ? e.message : String(e) })
        }
      }
      return failed
    },
    onSuccess: (failed) => {
      setNames(failed.map((f) => f.name).join(', '))
      refresh()
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); add.mutate() }
  const problems = add.data ?? []
  return (
    <section className="space-y-3">
      <h3 className="text-sm font-medium">{t('People with access')}</h3>
      <ul className="divide-y rounded-lg border">
        <li className="flex items-center gap-3 px-3 py-2">
          <Avatar name={me} />
          <span className="min-w-0 flex-1 truncate text-sm font-medium">{me} <span className="font-normal text-muted-foreground">({t('You')})</span></span>
          <span className="text-sm text-muted-foreground">{t('Owner')}</span>
        </li>
        {folder.members.map((m) => (
          <li key={m.user} className="flex items-center gap-3 px-3 py-2">
            <Avatar name={m.user} />
            <span className="min-w-0 flex-1 truncate text-sm font-medium">{m.user}</span>
            <AccessSelect canWrite={m.can_write} label={t('What {name} may do', { name: m.user })} onChange={(can_write) => save.mutate({ user: m.user, can_write })} />
            <Button size="icon" variant="ghost" aria-label={t('Stop sharing with {name}', { name: m.user })} onClick={() => remove.mutate({ user: m.user, can_write: m.can_write })}>
              <X className="size-4" />
            </Button>
          </li>
        ))}
      </ul>
      {folder.members.length === 0 && <p className="text-sm text-muted-foreground">{t('Nobody else. Add people to let them use this folder.')}</p>}
      <form onSubmit={submit} className="space-y-2">
        <Label htmlFor={`people-${folder.name}`}>{t('Add people')}</Label>
        <div className="flex flex-wrap gap-2">
          <Input id={`people-${folder.name}`} className="min-w-40 flex-1" placeholder={t('User names, separated by commas')} value={names} onChange={(e) => setNames(e.target.value)} />
          <AccessSelect canWrite={canWrite} onChange={setCanWrite} label={t('What they may do')} />
          <Button type="submit" disabled={!names.trim() || add.isPending}><UserPlus className="size-4" />{t('Share')}</Button>
        </div>
        {suggestions.length > 0 && (
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-xs text-muted-foreground">{t('Users on this server:')}</span>
            {suggestions.map((p) => (
              <button
                key={p.name}
                type="button"
                className="rounded-full border px-2 py-0.5 text-xs hover:bg-accent"
                onClick={() => setNames((n) => (n.split(/[\s,;]+/).includes(p.name) ? n : n.trim() ? `${n.trim().replace(/,$/, '')}, ${p.name}` : p.name))}
              >
                {p.name}
              </button>
            ))}
          </div>
        )}
        {problems.map((f) => <p key={f.name} role="alert" className="text-sm text-destructive">{f.why}</p>)}
      </form>
    </section>
  )
}

/** The command that syncs a folder of a computer with this shared folder, to copy. */
function AttachHint({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const command = `fafnirx shared attach ${folder.folder} <folder>`
  const copy = () => {
    void navigator.clipboard?.writeText(command).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    })
  }
  return (
    <div className="space-y-1">
      <p className="text-sm text-muted-foreground">{t('Not attached to any computer yet. From its tray menu (Shared folders), or with this command:')}</p>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 break-all rounded bg-muted px-2 py-1 text-xs">{command}</code>
        <Button size="icon" variant="ghost" aria-label={t('Copy the command')} onClick={copy}>
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
        </Button>
      </div>
    </div>
  )
}

/** Everything about one folder: who has it, which computers keep it, and the ways to end it. */
function FolderPanel({ folder, onClose }: { folder: SharedFolder | undefined; onClose: () => void }) {
  const t = useT()
  const refresh = useRefreshShared()
  const [asking, setAsking] = useState<'delete' | 'leave' | null>(null)
  const ended = () => { setAsking(null); onClose(); refresh() }
  const failed = (e: Error) => { setAsking(null); toast.error(e.message) }
  const remove = useMutation({ mutationFn: () => api.delete(`/api/shared/${encodeURIComponent(folder?.name ?? '')}`), onSuccess: ended, onError: failed })
  const leave = useMutation({ mutationFn: () => api.post('/api/shared/leave', { folder: folder?.folder }), onSuccess: ended, onError: failed })
  return (
    <Sheet open={folder !== undefined} onOpenChange={(o) => !o && onClose()}>
      <SheetContent className="w-full gap-0 overflow-y-auto sm:max-w-lg">
        {folder && (
          <>
            <SheetHeader>
              <SheetTitle className="break-all">{folder.folder}</SheetTitle>
              <SheetDescription>
                {folder.mine ? t('Yours') : t('Shared by {owner}', { owner: folder.owner })}
                {!folder.mine && <> · {folder.can_write ? t('Read and change') : t('Read only')}</>}
              </SheetDescription>
            </SheetHeader>
            <div className="space-y-6 px-4 pb-6">
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" variant="outline" asChild>
                  <Link to={`/files?device=${encodeURIComponent(folder.folder)}`}><FolderOpen className="size-4" />{t('Browse files')}</Link>
                </Button>
                <span className="text-sm text-muted-foreground"><Summary folder={folder} /></span>
              </div>
              {folder.mine && <People folder={folder} />}
              <section className="space-y-2">
                <h3 className="text-sm font-medium">{folder.mine ? t('Computers it is attached to') : t('Your computers that have it')}</h3>
                {folder.computers.length > 0 ? <SharedStateList rows={folder.computers} /> : <AttachHint folder={folder} />}
              </section>
              <section className="border-t pt-4">
                {folder.mine ? (
                  <Button variant="outline" onClick={() => setAsking('delete')}><Trash2 className="size-4" />{t('Delete')}</Button>
                ) : (
                  <Button variant="outline" onClick={() => setAsking('leave')}><LogOut className="size-4" />{t('Leave')}</Button>
                )}
              </section>
            </div>
            <Confirm
              open={asking === 'delete'}
              title={t('Delete {name}?', { name: folder.folder })}
              description={t('This removes the folder for good, with its trash and the history of its files. It can only be deleted once all its files are deleted.')}
              action={t('Delete')}
              destructive
              onConfirm={() => remove.mutate()}
              onCancel={() => setAsking(null)}
            />
            <Confirm
              open={asking === 'leave'}
              title={t('Stop using {name}?', { name: folder.folder })}
              description={t('Your computers stop syncing it. Nothing is deleted. {owner} can add you again.', { owner: folder.owner })}
              action={t('Leave')}
              destructive
              onConfirm={() => leave.mutate()}
              onCancel={() => setAsking(null)}
            />
          </>
        )}
      </SheetContent>
    </Sheet>
  )
}

/** Size and last change of a folder, in a line. */
function Summary({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const now = useNow()
  return (
    <>
      {folder.files === 0 ? t('Empty.') : t('{files} files, {size}', { files: count(folder.files), size: bytes(folder.bytes) })}
      {folder.changed_at !== null && <> · {t('changed {when}', { when: ago(folder.changed_at, now) })}</>}
    </>
  )
}

function Row({ folder, onOpen }: { folder: SharedFolder; onOpen: () => void }) {
  const t = useT()
  const shown = folder.members.slice(0, 3)
  return (
    <li className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 hover:bg-accent/40">
      <FolderSync className="size-5 shrink-0 text-muted-foreground" />
      <button type="button" onClick={onOpen} className="min-w-0 flex-1 basis-48 text-start">
        <span className="flex flex-wrap items-center gap-2">
          <span className="break-all font-medium"><span className="text-muted-foreground">{folder.owner}/</span>{folder.name}</span>
          {!folder.mine && <Badge variant="secondary">{t('Shared with you')}</Badge>}
          {!folder.mine && !folder.can_write && <Badge variant="outline">{t('Read only')}</Badge>}
        </span>
        <span className="block text-xs text-muted-foreground"><Summary folder={folder} /></span>
      </button>
      <div className="flex items-center gap-1.5 text-sm text-muted-foreground" title={folder.members.map((m) => m.user).join(', ')}>
        {folder.mine ? (
          folder.members.length === 0 ? (
            t('Only you')
          ) : (
            <>
              <span className="flex -space-x-2">{shown.map((m) => <Avatar key={m.user} name={m.user} />)}</span>
              {t('{count} people', { count: folder.members.length })}
            </>
          )
        ) : (
          t('From {owner}', { owner: folder.owner })
        )}
      </div>
      <div className="w-32">{syncBadge(folder, t)}</div>
      <div className="flex items-center gap-1">
        <Button size="sm" variant="outline" onClick={onOpen}>{folder.mine ? t('Share') : t('Details')}</Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild><Button size="icon" variant="ghost" aria-label={t('More')}><MoreHorizontal className="size-4" /></Button></DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem asChild><Link to={`/files?device=${encodeURIComponent(folder.folder)}`}><FolderOpen className="size-4" />{t('Browse files')}</Link></DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </li>
  )
}

export default function Shared() {
  const t = useT()
  const [show, setShow] = useState<Show>('all')
  const [search, setSearch] = useState('')
  const [opened, setOpened] = useState<string | null>(null)
  const { data, error } = useQuery({ queryKey: ['shared'], queryFn: () => api.get<SharedFolder[]>('/api/shared') })
  const shown = useMemo(() => {
    const word = search.trim().toLowerCase()
    return (data ?? []).filter((f) => (show === 'all' || (show === 'mine') === f.mine) && (word === '' || f.folder.includes(word)))
  }, [data, show, search])
  const mine = data?.filter((f) => f.mine).length ?? 0
  const filters: { id: Show; label: string }[] = [
    { id: 'all', label: t('All ({count})', { count: data?.length ?? 0 }) },
    { id: 'mine', label: t('Yours ({count})', { count: mine }) },
    { id: 'others', label: t('Shared with you ({count})', { count: (data?.length ?? 0) - mine }) },
  ]
  return (
    <div>
      <PageHeader
        title={t('Shared folders')}
        description={t('Folders that belong to you, not to one computer. Attach one to a folder from the tray menu of each computer and it stays the same on all of them. Other users can use one only when you add them here.')}
        actions={<AddShared onMade={setOpened} />}
      />
      {error ? <ErrorNote error={error} /> : !data ? (
        <Skeleton className="h-48" />
      ) : data.length === 0 ? (
        <Empty title={t('No shared folders')}>{t('Make one, then attach a folder of each computer to it from the tray menu.')}</Empty>
      ) : (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-2">
            <Input className="w-56" type="search" aria-label={t('Search shared folders')} placeholder={t('Search')} value={search} onChange={(e) => setSearch(e.target.value)} />
            {filters.map((f) => (
              <Button key={f.id} size="sm" variant={show === f.id ? 'secondary' : 'ghost'} onClick={() => setShow(f.id)}>{f.label}</Button>
            ))}
          </div>
          {shown.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t('No shared folder matches.')}</p>
          ) : (
            <ul className="divide-y rounded-lg border bg-card">
              {shown.map((f) => <Row key={f.folder} folder={f} onOpen={() => setOpened(f.folder)} />)}
            </ul>
          )}
        </div>
      )}
      <FolderPanel folder={data?.find((f) => f.folder === opened)} onClose={() => setOpened(null)} />
    </div>
  )
}
