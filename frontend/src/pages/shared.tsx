import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Copy, FolderOpen, LogOut, Plus, Trash2, X } from 'lucide-react'
import { useMemo, useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { Confirm } from '@/components/confirm'
import { SharedStateList } from '@/components/shared-state'
import { Empty, ErrorNote, PageHeader, SELECT } from '@/components/common'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, bytes, count } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'
import type { SharedFolder } from '@/lib/types'

type Show = 'all' | 'mine' | 'others'

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

/** A folder's name with its owner in front, the way the computers show it. */
function FolderName({ folder }: { folder: SharedFolder }) {
  return (
    <span className="min-w-0 break-all">
      <span className="text-muted-foreground">{folder.owner}/</span>
      {folder.name}
    </span>
  )
}

function AddShared() {
  const t = useT()
  const me = useSession()?.user?.name
  const refresh = useRefreshShared()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const full = `${me ?? ''}/${name.trim().toLowerCase()}`
  const create = useMutation({
    mutationFn: () => api.post('/api/shared', { name }),
    onSuccess: () => {
      toast.success(t('Shared folder {name} made. Attach it to a folder from the tray menu of a computer.', { name: full }))
      setOpen(false)
      setName('')
      refresh()
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

function Members({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const refresh = useRefreshShared()
  const [user, setUser] = useState('')
  const [canWrite, setCanWrite] = useState(true)
  const [removing, setRemoving] = useState<string | null>(null)
  const members = `/api/shared/${encodeURIComponent(folder.name)}/members`
  // Adding a user and changing what they may do are the same request.
  const save = useMutation({
    mutationFn: (m: { user: string; can_write: boolean }) => api.post(members, m),
    onSuccess: refresh,
  })
  const remove = useMutation({
    mutationFn: (name: string) => api.delete(`${members}/${encodeURIComponent(name)}`),
    onSuccess: refresh,
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => { e.preventDefault(); save.mutate({ user, can_write: canWrite }, { onSuccess: () => setUser('') }) }
  return (
    <div className="space-y-3">
      <p className="text-sm font-medium">{t('Shared with other users')}</p>
      {folder.members.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t('Nobody. Only you, on all your computers.')}</p>
      ) : (
        <ul className="space-y-2">
          {folder.members.map((m) => (
            <li key={m.user} className="flex flex-wrap items-center gap-2">
              <span className="min-w-24 font-medium">{m.user}</span>
              <AccessSelect
                canWrite={m.can_write}
                label={t('What {name} may do', { name: m.user })}
                onChange={(can_write) => save.mutate({ user: m.user, can_write }, { onError: (e) => toast.error(e.message) })}
              />
              <Button size="icon" variant="ghost" aria-label={t('Stop sharing with {name}', { name: m.user })} onClick={() => setRemoving(m.user)}>
                <X className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submit} className="flex flex-wrap items-end gap-2">
        <div className="space-y-1">
          <Label htmlFor={`member-${folder.folder}`}>{t('Add a user')}</Label>
          <Input id={`member-${folder.folder}`} className="w-48" placeholder={t('User name')} value={user} onChange={(e) => setUser(e.target.value)} />
        </div>
        <AccessSelect canWrite={canWrite} onChange={setCanWrite} label={t('What they may do')} />
        <Button type="submit" disabled={!user.trim() || save.isPending}>{t('Share')}</Button>
      </form>
      {save.error && <p role="alert" className="text-sm text-destructive">{save.error.message}</p>}
      <Confirm
        open={removing !== null}
        title={t('Stop sharing {folder} with {name}?', { folder: folder.folder, name: removing ?? '' })}
        description={t('Their computers stop syncing it. Nothing is deleted: the files stay in the folder, on the master, and on their computers.')}
        action={t('Stop sharing')}
        destructive
        onConfirm={() => { if (removing) remove.mutate(removing); setRemoving(null) }}
        onCancel={() => setRemoving(null)}
      />
    </div>
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
      <p className="text-sm font-medium">{t('Use it on a computer')}</p>
      <p className="text-sm text-muted-foreground">{t('From its tray menu (Shared folders), or with this command:')}</p>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 break-all rounded bg-muted px-2 py-1 text-xs">{command}</code>
        <Button size="icon" variant="ghost" aria-label={t('Copy the command')} onClick={copy}>
          {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
        </Button>
      </div>
    </div>
  )
}

function Folder({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const now = useNow()
  const refresh = useRefreshShared()
  const [asking, setAsking] = useState<'delete' | 'leave' | null>(null)
  const done = () => { setAsking(null); refresh() }
  const failed = (e: Error) => { setAsking(null); toast.error(e.message) }
  const remove = useMutation({ mutationFn: () => api.delete(`/api/shared/${encodeURIComponent(folder.name)}`), onSuccess: done, onError: failed })
  const leave = useMutation({ mutationFn: () => api.post('/api/shared/leave', { folder: folder.folder }), onSuccess: done, onError: failed })
  return (
    <Card>
      <CardHeader className="grid-cols-[1fr_auto] items-center">
        <CardTitle className="flex flex-wrap items-center gap-2">
          <FolderName folder={folder} />
          {folder.mine ? <Badge variant="outline">{t('Yours')}</Badge> : <Badge variant="secondary">{t('Shared with you')}</Badge>}
          {!folder.mine && !folder.can_write && <Badge variant="outline">{t('Read only')}</Badge>}
        </CardTitle>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" asChild>
            <Link to={`/files?device=${encodeURIComponent(folder.folder)}`}><FolderOpen className="size-4" />{t('Browse files')}</Link>
          </Button>
          {folder.mine ? (
            <Button size="sm" variant="outline" onClick={() => setAsking('delete')}><Trash2 className="size-4" />{t('Delete')}</Button>
          ) : (
            <Button size="sm" variant="outline" onClick={() => setAsking('leave')}><LogOut className="size-4" />{t('Leave')}</Button>
          )}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">
          {folder.files === 0 ? t('Empty.') : t('{files} files, {size}', { files: count(folder.files), size: bytes(folder.bytes) })}
          {folder.changed_at !== null && <> · {t('changed {when}', { when: ago(folder.changed_at, now) })}</>}
          {folder.mine && folder.members.length > 0 && <> · {t('shared with {count} users', { count: folder.members.length })}</>}
        </p>
        {folder.computers.length > 0 ? (
          <div className="space-y-1">
            <p className="text-sm font-medium">{folder.mine ? t('Computers it is attached to') : t('Your computers that have it')}</p>
            <SharedStateList rows={folder.computers} />
          </div>
        ) : (
          <AttachHint folder={folder} />
        )}
        {folder.mine && <Members folder={folder} />}
      </CardContent>
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
    </Card>
  )
}

export default function Shared() {
  const t = useT()
  const [show, setShow] = useState<Show>('all')
  const [search, setSearch] = useState('')
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
        actions={<AddShared />}
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
            shown.map((f) => <Folder key={f.folder} folder={f} />)
          )}
        </div>
      )}
    </div>
  )
}
