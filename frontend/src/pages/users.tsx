import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { dateTime } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'
import type { Role, UserInfo } from '@/lib/types'

const ROLES: [Role, string, string][] = [
  ['admin', msg('Administrator'), msg('Everything: users, settings, mirrors, sources, and every user\'s files.')],
  ['normal', msg('Normal'), msg('Devices and files of their own, and read and write on the master files.')],
  ['readonly', msg('Read only'), msg('Reads the master files. No devices, albums or favorites.')],
]

const SELECT = 'h-9 rounded-md border bg-background px-2 text-sm'

function RoleSelect({ id, value, onChange }: { id?: string; value: Role; onChange: (role: Role) => void }) {
  const t = useT()
  return (
    <select id={id} className={SELECT} value={value} onChange={(e) => onChange(e.target.value as Role)}>
      {ROLES.map(([role, label]) => <option key={role} value={role}>{t(label)}</option>)}
    </select>
  )
}

function AddUser() {
  const t = useT()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [role, setRole] = useState<Role>('normal')
  const [password, setPassword] = useState('')
  const create = useMutation({
    mutationFn: () => api.post('/api/users', { name, role, password }),
    onSuccess: () => {
      setOpen(false)
      setName('')
      setPassword('')
      void client.invalidateQueries({ queryKey: ['users'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />{t('Add user')}</Button></DialogTrigger>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Add a user')}</DialogTitle>
            <DialogDescription>{t("The name is the user's folder on the master (files/<name>/) and cannot change later.")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="user-name">{t('Name')}</Label>
            <Input id="user-name" autoFocus placeholder="alice" value={name} onChange={(e) => setName(e.target.value)} />
            <p className="text-xs text-muted-foreground">{t('Lowercase letters, digits, dot, underscore and dash; up to 63 characters.')}</p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="user-role">{t('Role')}</Label>
            <RoleSelect id="user-role" value={role} onChange={setRole} />
            <p className="text-xs text-muted-foreground">{t(ROLES.find((r) => r[0] === role)?.[2] ?? '')}</p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="user-password">{t('Password')}</Label>
            <Input id="user-password" type="password" autoComplete="new-password" value={password} onChange={(e) => setPassword(e.target.value)} />
            <p className="text-xs text-muted-foreground">{t('At least 8 characters.')}</p>
            {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          </div>
          <DialogFooter><Button type="submit" disabled={!name.trim() || password.length < 8 || create.isPending}>{t('Create')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function ResetPassword({ user }: { user: UserInfo }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [password, setPassword] = useState('')
  const reset = useMutation({
    mutationFn: () => api.patch(`/api/users/${encodeURIComponent(user.name)}`, { password }),
    onSuccess: () => {
      toast.success(t('New password set for {name}. Their logins have ended.', { name: user.name }))
      setOpen(false)
      setPassword('')
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); reset.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); reset.reset() }}>
      <DialogTrigger asChild><Button size="sm" variant="outline">{t('New password')}</Button></DialogTrigger>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader><DialogTitle>{t('New password for {name}', { name: user.name })}</DialogTitle></DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="reset-password">{t('Password')}</Label>
            <Input id="reset-password" type="password" autoComplete="new-password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} />
            <p className="text-xs text-muted-foreground">{t('At least 8 characters.')}</p>
            {reset.error && <p role="alert" className="text-sm text-destructive">{reset.error.message}</p>}
          </div>
          <DialogFooter><Button type="submit" disabled={password.length < 8 || reset.isPending}>{t('Set password')}</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function Row({ user, me }: { user: UserInfo; me: string }) {
  const t = useT()
  const client = useQueryClient()
  const change = useMutation({
    mutationFn: (body: { role?: Role; disabled?: boolean }) => api.patch(`/api/users/${encodeURIComponent(user.name)}`, body),
    onSuccess: () => void client.invalidateQueries({ queryKey: ['users'] }),
    onError: (e) => toast.error(e.message),
  })
  return (
    <TableRow className={user.disabled ? 'opacity-60' : undefined}>
      <TableCell>
        <span className="font-medium">{user.name}</span>
        {user.name === me && <Badge variant="secondary" className="ms-2">{t('You')}</Badge>}
        {user.disabled && <Badge variant="outline" className="ms-2">{t('Disabled')}</Badge>}
      </TableCell>
      <TableCell><RoleSelect value={user.role} onChange={(role) => change.mutate({ role })} /></TableCell>
      <TableCell className="hidden sm:table-cell text-right tabular-nums">{user.devices}</TableCell>
      <TableCell className="hidden md:table-cell text-muted-foreground">{dateTime(user.created_at)}</TableCell>
      <TableCell>
        <div className="flex justify-end gap-2">
          <ResetPassword user={user} />
          <Button size="sm" variant="outline" disabled={change.isPending} onClick={() => change.mutate({ disabled: !user.disabled })}>
            {user.disabled ? t('Enable') : t('Disable')}
          </Button>
        </div>
      </TableCell>
    </TableRow>
  )
}

export default function Users() {
  const t = useT()
  const me = useSession()?.user?.name ?? ''
  const { data, error } = useQuery({ queryKey: ['users'], queryFn: () => api.get<UserInfo[]>('/api/users') })
  return (
    <div>
      <PageHeader
        title={t('Users')}
        description={t('Who can log in. Each user has a folder and devices of their own; the master files are shared by everybody.')}
        actions={<AddUser />}
      />
      {error ? <ErrorNote error={error} /> : !data ? (
        <Skeleton className="h-48" />
      ) : data.length === 0 ? (
        <Empty title={t('No users')}>{t('Add a user to share this master.')}</Empty>
      ) : (
        <div className="rounded-lg border bg-card">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('Name')}</TableHead>
                <TableHead>{t('Role')}</TableHead>
                <TableHead className="hidden sm:table-cell text-right">{t('Devices')}</TableHead>
                <TableHead className="hidden md:table-cell">{t('Added')}</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>{data.map((u) => <Row key={u.name} user={u} me={me} />)}</TableBody>
          </Table>
        </div>
      )}
    </div>
  )
}
