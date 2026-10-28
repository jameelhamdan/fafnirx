import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FolderOpen, Plus, Trash2, X } from 'lucide-react'
import { useState, type FormEvent } from 'react'
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
import { api } from '@/lib/api'
import { dateTime } from '@/lib/format'
import { useT } from '@/lib/i18n'
import type { SharedFolder } from '@/lib/types'

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

function AddShared() {
  const t = useT()
  const refresh = useRefreshShared()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post('/api/shared', { name }),
    onSuccess: () => {
      toast.success(t('Shared folder {name} made. Attach it to a folder from the tray menu of a computer.', { name }))
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
            <Input id="shared-name" autoFocus placeholder="family" value={name} onChange={(e) => setName(e.target.value)} />
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
              <span className="font-medium">{m.user}</span>
              <AccessSelect
                canWrite={m.can_write}
                label={t('What {name} may do', { name: m.user })}
                onChange={(can_write) => save.mutate({ user: m.user, can_write }, { onError: (e) => toast.error(e.message) })}
              />
              <Button size="icon" variant="ghost" aria-label={t('Stop sharing with {name}', { name: m.user })} onClick={() => remove.mutate(m.user)}>
                <X className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submit} className="flex flex-wrap items-end gap-2">
        <div className="space-y-1">
          <Label htmlFor={`member-${folder.name}`}>{t('Add a user')}</Label>
          <Input id={`member-${folder.name}`} className="w-48" placeholder={t('User name')} value={user} onChange={(e) => setUser(e.target.value)} />
        </div>
        <AccessSelect canWrite={canWrite} onChange={setCanWrite} label={t('What they may do')} />
        <Button type="submit" disabled={!user.trim() || save.isPending}>{t('Share')}</Button>
      </form>
      {save.error && <p role="alert" className="text-sm text-destructive">{save.error.message}</p>}
    </div>
  )
}

function Folder({ folder }: { folder: SharedFolder }) {
  const t = useT()
  const refresh = useRefreshShared()
  const [asking, setAsking] = useState(false)
  const remove = useMutation({
    mutationFn: () => api.delete(`/api/shared/${encodeURIComponent(folder.name)}`),
    onSuccess: () => { setAsking(false); refresh() },
    onError: (e) => { setAsking(false); toast.error(e.message) },
  })
  return (
    <Card>
      <CardHeader className="grid-cols-[1fr_auto] items-center">
        <CardTitle className="flex flex-wrap items-center gap-2">
          <span>{folder.folder}</span>
          {!folder.mine && <Badge variant="secondary">{t('Shared with you')}</Badge>}
          {!folder.mine && !folder.can_write && <Badge variant="outline">{t('Read only')}</Badge>}
        </CardTitle>
        <div className="flex gap-2">
          <Button size="sm" variant="outline" asChild>
            <Link to={`/files?device=${encodeURIComponent(folder.folder)}`}><FolderOpen className="size-4" />{t('Browse files')}</Link>
          </Button>
          {folder.mine && (
            <Button size="sm" variant="outline" onClick={() => setAsking(true)}>
              <Trash2 className="size-4" />{t('Delete')}
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm text-muted-foreground">{t('Made {when}.', { when: dateTime(folder.created_at) })}</p>
        {folder.computers.length > 0 && (
          <div className="space-y-1">
            <p className="text-sm font-medium">{folder.mine ? t('Computers it is attached to') : t('Your computers that have it')}</p>
            <SharedStateList rows={folder.computers} />
          </div>
        )}
        {folder.mine && <Members folder={folder} />}
      </CardContent>
      <Confirm
        open={asking}
        title={t('Delete {name}?', { name: folder.name })}
        description={t('This removes the folder for good, with its trash and the history of its files. It can only be deleted once all its files are deleted.')}
        action={t('Delete')}
        destructive
        onConfirm={() => remove.mutate()}
        onCancel={() => setAsking(false)}
      />
    </Card>
  )
}

export default function Shared() {
  const t = useT()
  const { data, error } = useQuery({ queryKey: ['shared'], queryFn: () => api.get<SharedFolder[]>('/api/shared') })
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
        <div className="space-y-4">{data.map((f) => <Folder key={f.folder} folder={f} />)}</div>
      )}
    </div>
  )
}
