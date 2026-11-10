import { useQuery } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { useMemo, useState } from 'react'
import { Empty, ErrorNote, PageHeader, SELECT } from '@/components/common'
import { DataTable, TableToolbar } from '@/components/data-table'
import { matches, useTableView, type TableView } from '@/lib/table-view'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { FormDialog } from '@/components/form-dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { dateTime } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'
import type { Role, UserInfo } from '@/lib/types'
import { useApiMutation } from '@/lib/mutation'

const ROLES: [Role, string, string][] = [
  ['admin', msg('Administrator'), msg('Everything: users, settings, mirrors, sources, and every user\'s files.')],
  ['normal', msg('Normal'), msg('Devices and files of their own, and read and write on the master files.')],
  ['readonly', msg('Read only'), msg('Reads the master files. No devices, albums or favorites.')],
]

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
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [role, setRole] = useState<Role>('normal')
  const [password, setPassword] = useState('')
  const create = useApiMutation(() => api.post('/api/users', { name, role, password }), {
    invalidate: [['users']],
    onSuccess: () => {
      setOpen(false)
      setName('')
      setPassword('')
    },
  })

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { setOpen(o); create.reset() }}
      trigger={<Button><Plus className="size-4" />{t('Add user')}</Button>}
      title={t('Add a user')}
      description={t("The name is the user's folder on the master (files/<name>/) and cannot change later.")}
      onSubmit={() => create.mutate()}
      submit={t('Create')}
      disabled={!name.trim() || password.length < 8}
      pending={create.isPending}
      error={create.error?.message}
    >
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
      </div>
    </FormDialog>
  )
}

function ResetPassword({ user }: { user: UserInfo }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [password, setPassword] = useState('')
  const reset = useApiMutation(() => api.patch(`/api/users/${encodeURIComponent(user.name)}`, { password }), {
    success: t('New password set for {name}. Their logins have ended.', { name: user.name }),
    onSuccess: () => {
      setOpen(false)
      setPassword('')
    },
  })

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { setOpen(o); reset.reset() }}
      trigger={<Button size="xs" variant="outline">{t('New password')}</Button>}
      title={t('New password for {name}', { name: user.name })}
      onSubmit={() => reset.mutate()}
      submit={t('Set password')}
      disabled={password.length < 8}
      pending={reset.isPending}
      error={reset.error?.message}
    >
      <div className="space-y-2">
        <Label htmlFor="reset-password">{t('Password')}</Label>
        <Input id="reset-password" type="password" autoComplete="new-password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} />
        <p className="text-xs text-muted-foreground">{t('At least 8 characters.')}</p>
      </div>
    </FormDialog>
  )
}

function Row({ user, me, view }: { user: UserInfo; me: string; view: TableView }) {
  const t = useT()
  const change = useApiMutation((body: { role?: Role; disabled?: boolean }) => api.patch(`/api/users/${encodeURIComponent(user.name)}`, body), {
    invalidate: [['users']],
    toastError: true,
  })
  return (
    <TableRow className={user.disabled ? 'opacity-60' : undefined}>
      <TableCell>
        <span className="num text-[13px] font-medium">{user.name}</span>
        {user.name === me && <Badge variant="secondary" className="ms-2">{t('You')}</Badge>}
        {user.disabled && <Badge variant="outline" className="ms-2">{t('Disabled')}</Badge>}
      </TableCell>
      <TableCell><RoleSelect value={user.role} onChange={(role) => change.mutate({ role })} /></TableCell>
      {view.show('devices') && <TableCell className="num hidden text-end sm:table-cell">{user.devices}</TableCell>}
      {view.show('added') && <TableCell className="num hidden text-[12px] text-muted-foreground md:table-cell">{dateTime(user.created_at)}</TableCell>}
      <TableCell>
        <div className="flex justify-end gap-2">
          <ResetPassword user={user} />
          <Button size="xs" variant="outline" disabled={change.isPending} onClick={() => change.mutate({ disabled: !user.disabled })}>
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
  const [filter, setFilter] = useState('')
  const [role, setRole] = useState('')
  const columns = useMemo(() => [{ id: 'name', label: t('Name'), locked: true }, { id: 'role', label: t('Role'), locked: true }, { id: 'devices', label: t('Devices') }, { id: 'added', label: t('Added') }], [t])
  const view = useTableView('users', columns)
  const shown = (data ?? []).filter((u) => (role === '' || u.role === role) && matches(filter, u.name))
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
        <DataTable
          view={view}
          toolbar={
            <TableToolbar view={view} filter={filter} onFilter={setFilter} placeholder={t('Filter users')} shown={shown.length} total={data.length}>
              <select className="h-7 rounded-md border border-input bg-background px-1.5 text-[13px] focus-visible:border-ring focus-visible:outline-none" value={role} onChange={(e) => setRole(e.target.value)} aria-label={t('Role')}>
                <option value="">{t('Any role')}</option>
                {ROLES.map(([value, label]) => <option key={value} value={value}>{t(label)}</option>)}
              </select>
            </TableToolbar>
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('Name')}</TableHead>
                <TableHead>{t('Role')}</TableHead>
                {view.show('devices') && <TableHead className="hidden text-end sm:table-cell">{t('Devices')}</TableHead>}
                {view.show('added') && <TableHead className="hidden md:table-cell">{t('Added')}</TableHead>}
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.length === 0 && <TableRow className="hover:bg-transparent"><TableCell colSpan={5} className="py-6 text-center text-muted-foreground">{t('No user matches.')}</TableCell></TableRow>}
              {shown.map((u) => <Row key={u.name} user={u} me={me} view={view} />)}
            </TableBody>
          </Table>
        </DataTable>
      )}
    </div>
  )
}
