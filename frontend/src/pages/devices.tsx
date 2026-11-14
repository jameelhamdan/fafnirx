import { ChevronRight, Plus } from 'lucide-react'
import { KEYS } from '@/services/keys'
import { useMemo, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { ActivityView, Picker, Empty, ErrorNote, PageHeader, StatusDot } from '@/components/common'
import { DataTable, TableToolbar } from '@/components/data-table'
import { matches, useTableView, type TableView } from '@/lib/table-view'
import { EnrollDialog } from '@/components/enroll-dialog'
import { AddFolder, FolderPanel, FoldersSection, SourceBadge } from '@/components/folders-section'
import { AddMasterFolder } from '@/components/master-folders'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { FormDialog } from '@/components/form-dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useNow } from '@/hooks/use-now'
import { createDevice, useDevice, useDevices } from '@/services/devices'
import { useFolders } from '@/services/folders'
import { useUsers } from '@/services/users'
import { ago, bytes, count } from '@/lib/format'
import type { DeviceDetail, DeviceSummary, Folder } from '@/lib/types'
import { useT } from '@/lib/i18n'
import { folderLink } from '@/lib/links'
import { deviceAddress, useRole, useSession } from '@/services/session'
import { useApiMutation } from '@/lib/mutation'

function AddDeviceDialog({ onCreated }: { onCreated: (made: DeviceDetail) => void }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [owner, setOwner] = useState('')
  const session = useSession()
  const isAdmin = session?.user?.role === 'admin'
  // An administrator can add a device for any user who can have one.
  const users = useUsers(isAdmin && open)
  const create = useApiMutation(() => createDevice(name, owner || undefined), {
    inline: true,
    invalidate: [KEYS.devices],
    onSuccess: (made) => {
      setOpen(false)
      setName('')
      onCreated(made)
    },
  })

  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { setOpen(o); create.reset() }}
      trigger={<Button><Plus className="size-4" />{t('Add device')}</Button>}
      title={t('Add a device')}
      description={t("The name is the device's folder on the master (files/<user>/<name>/) and cannot change later.")}
      onSubmit={() => create.mutate()}
      submit={t('Create')}
      disabled={!name.trim()}
      pending={create.isPending}
      error={create.error?.message}
    >
      <div className="space-y-2">
        <Label htmlFor="device-name">{t('Name')}</Label>
        <Input id="device-name" autoFocus placeholder="laptop" value={name} onChange={(e) => setName(e.target.value)} />
        <p className="text-xs text-muted-foreground">{t('Lowercase letters, digits, dot, underscore and dash; up to 63 characters.')}</p>
        {isAdmin && users.data && (
          <div className="space-y-2 pt-2">
            <Label htmlFor="device-owner">{t('For user')}</Label>
            <Picker
              id="device-owner"
              className="w-full"
              value={owner}
              onChange={setOwner}
              options={[
                ['', t('Me ({name})', { name: session?.user?.name ?? '' })],
                ...users.data.filter((u) => !u.disabled && u.role !== 'readonly' && u.name !== session?.user?.name).map((u): [string, string] => [u.name, u.name]),
              ]}
            />
          </div>
        )}
      </div>
    </FormDialog>
  )
}

/** The root folders of a device, as it last said, with what each one is: shared, filled by a source, read only. */
function DeviceFolders({ device, folders, onOpen }: { device: DeviceDetail['device']; folders: Folder[]; onOpen: (folder: string) => void }) {
  const t = useT()
  const now = useNow()
  const { data, error } = useDevice(device.key)
  if (error) return <ErrorNote error={error} />
  if (!data) return <Skeleton className="h-10" />
  if (data.folders.length === 0) return <p className="text-sm text-muted-foreground">{t('No folder is attached to this device yet.')}</p>
  return (
    <ul className="divide-y">
      {data.folders.map((r) => {
        const folder = folders.find((f) => f.key === r.name)
        const trouble = r.last_error !== '' || r.failing > 0
        return (
          <li key={r.name} className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-sm">
            <Link to={folderLink(r.name)} className="font-medium hover:underline">{folder?.mine ? folder.name : r.name}</Link>
            {r.last_cycle === 0 ? <Badge variant="outline">{t('Not synced yet')}</Badge> : trouble ? <Badge variant="destructive">{t('Needs attention')}</Badge> : <Badge variant="secondary">{t('In step')}</Badge>}
            {folder && <SourceBadge folder={folder} />}
            {folder?.shared && <Badge variant="outline">{t('Shared')}</Badge>}
            {folder && !folder.can_write && <Badge variant="outline">{t('Read only')}</Badge>}
            <span className="text-muted-foreground">{t('{files} files, {size}', { files: count(r.files), size: bytes(r.bytes) })}</span>
            {r.last_cycle > 0 && <span className="text-muted-foreground">{t('Synced {when}', { when: ago(r.last_cycle, now) })}</span>}
            {folder && <Button size="sm" variant="ghost" className="ms-auto" onClick={() => onOpen(folder.key)}>{t('Details')}</Button>}
            {r.last_error && <p className="basis-full text-xs text-destructive">{r.last_error}</p>}
          </li>
        )
      })}
    </ul>
  )
}

function Row({ d, now, view, showUser, folders, onOpen }: { d: DeviceSummary; now: number; view: TableView; showUser: boolean; folders: Folder[]; onOpen: (folder: string) => void }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <>
      <TableRow>
        <TableCell>
          <div className="flex items-center gap-2">
            <Button size="icon" variant="ghost" className="size-7" aria-expanded={open} aria-label={open ? t('Hide folders') : t('Show folders')} onClick={() => setOpen(!open)}>
              <ChevronRight className={`size-4 transition-transform ${open ? 'rotate-90' : 'rtl:rotate-180'}`} />
            </Button>
            <Link to={`/devices/${deviceAddress(d.key)}`} className="num text-compact font-medium hover:underline">{showUser ? d.key : d.name}</Link>
          </div>
          {d.activity && <div className="mt-2 max-w-sm ps-9"><ActivityView activity={d.activity} /></div>}
        </TableCell>
        <TableCell><StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? t('Revoked') : d.online ? t('Online') : t('Offline')} /></TableCell>
        {view.show('os') && <TableCell className="hidden md:table-cell text-muted-foreground">{d.os ?? '—'}</TableCell>}
        {view.show('version') && <TableCell className="num hidden text-xs text-muted-foreground lg:table-cell">{d.client_version ?? '—'}</TableCell>}
        {view.show('ip') && <TableCell className="num hidden text-xs text-muted-foreground lg:table-cell">{d.ip ?? '—'}</TableCell>}
        {view.show('files') && <TableCell className="num text-end">{count(d.files)}</TableCell>}
        {view.show('size') && <TableCell className="num text-end">{bytes(d.bytes)}</TableCell>}
        {view.show('seen') && <TableCell className="num hidden text-end text-muted-foreground sm:table-cell">{ago(d.last_seen, now)}</TableCell>}
      </TableRow>
      {open && (
        <TableRow className="hover:bg-transparent">
          <TableCell colSpan={9} className="bg-muted/30 ps-11"><DeviceFolders device={d} folders={folders} onOpen={onOpen} /></TableCell>
        </TableRow>
      )}
    </>
  )
}

/** The heading of one of the two lists on the page, with the buttons that add to it. */
function SectionHeader({ title, description, actions }: { title: string; description: string; actions: ReactNode }) {
  return (
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 className="text-base font-semibold tracking-tight">{title}</h2>
        <p className="mt-0.5 max-w-3xl text-compact text-muted-foreground">{description}</p>
      </div>
      <div className="flex flex-wrap items-center gap-2">{actions}</div>
    </div>
  )
}

export default function Devices() {
  const t = useT()
  const now = useNow()
  const [made, setMade] = useState<DeviceDetail | null>(null)
  const [opened, setOpened] = useState<string | null>(null)
  const admin = useRole() === 'admin'
  const devices = useDevices()
  const folders = useFolders()
  const all = folders.data ?? []
  const [filter, setFilter] = useState('')
  const [only, setOnly] = useState('')
  const columns = useMemo(
    () => [
      { id: 'name', label: t('Name'), locked: true },
      { id: 'status', label: t('Status'), locked: true },
      { id: 'os', label: t('System') },
      { id: 'version', label: t('Client') },
      { id: 'ip', label: t('Address') },
      { id: 'files', label: t('Files') },
      { id: 'size', label: t('Size') },
      { id: 'seen', label: t('Last seen') },
    ],
    [t],
  )
  const view = useTableView('devices', columns)
  const shown = (devices.data ?? []).filter((d) => (only === '' || (only === 'revoked' ? d.revoked : only === 'online' ? d.online && !d.revoked : !d.online && !d.revoked)) && matches(filter, d.key, d.os, d.hostname, d.ip))

  return (
    <div className="space-y-6">
      <PageHeader title={t('Devices & folders')} />
      <section className="space-y-3">
        <SectionHeader
          title={t('Devices')}
          description={t('Devices that send their folders here. Open one to see its folders.')}
          actions={<AddDeviceDialog onCreated={setMade} />}
        />
        {devices.error ? <ErrorNote error={devices.error} /> : !devices.data ? (
          <Skeleton className="h-48" />
        ) : devices.data.length === 0 ? (
          <Empty title={t('No devices yet')}>{t('Add a device to get a token and the command that connects it.')}</Empty>
        ) : (
          <DataTable
            view={view}
            toolbar={
              <TableToolbar view={view} filter={filter} onFilter={setFilter} placeholder={t('Filter by name, system or address')} shown={shown.length} total={devices.data.length}>
                <Picker
                  size="sm"
                  className="w-40"
                  label={t('Status')}
                  value={only}
                  onChange={setOnly}
                  options={[['', t('Any status')], ['online', t('Online')], ['offline', t('Offline')], ['revoked', t('Revoked')]]}
                />
              </TableToolbar>
            }
          >
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t('Name')}</TableHead>
                  <TableHead>{t('Status')}</TableHead>
                  {view.show('os') && <TableHead className="hidden md:table-cell">{t('System')}</TableHead>}
                  {view.show('version') && <TableHead className="hidden lg:table-cell">{t('Client')}</TableHead>}
                  {view.show('ip') && <TableHead className="hidden lg:table-cell">{t('Address')}</TableHead>}
                  {view.show('files') && <TableHead className="text-end">{t('Files')}</TableHead>}
                  {view.show('size') && <TableHead className="text-end">{t('Size')}</TableHead>}
                  {view.show('seen') && <TableHead className="hidden text-end sm:table-cell">{t('Last seen')}</TableHead>}
                </TableRow>
              </TableHeader>
              <TableBody>
                {shown.length === 0 && <TableRow className="hover:bg-transparent"><TableCell colSpan={9} className="py-6 text-center text-muted-foreground">{t('No device matches.')}</TableCell></TableRow>}
                {shown.map((d) => <Row key={d.key} d={d} now={now} view={view} showUser={admin} folders={all} onOpen={setOpened} />)}
              </TableBody>
            </Table>
          </DataTable>
        )}
      </section>
      <section className="space-y-3">
        <SectionHeader
          title={t('Folders')}
          description={t('Folders that belong to you, or to the master files, not to one device. Attach one to a folder of each device and it stays the same on all of them. Other users can use one of yours only when you add them here.')}
          actions={<>{admin && <AddMasterFolder />}<AddFolder onMade={setOpened} /></>}
        />
        {folders.error ? <ErrorNote error={folders.error} /> : !folders.data ? (
          <Skeleton className="h-32" />
        ) : all.length === 0 ? (
          <Empty title={t('No folders')}>{t('Make one, then attach a folder of each device to it.')}</Empty>
        ) : (
          <FoldersSection folders={all} onOpen={setOpened} />
        )}
      </section>
      <FolderPanel folder={all.find((f) => f.key === opened)} onClose={() => setOpened(null)} />
      <EnrollDialog name={made?.device.key ?? ''} token={made?.token ?? null} onClose={() => setMade(null)} />
    </div>
  )
}
