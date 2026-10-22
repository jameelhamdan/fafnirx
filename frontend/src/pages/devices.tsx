import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { ActivityView, Empty, ErrorNote, KindBadge, PageHeader, StatusDot } from '@/components/common'
import { EnrollDialog } from '@/components/enroll-dialog'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, bytes, count } from '@/lib/format'
import type { DeviceDetail, DeviceSummary } from '@/lib/types'
import { useT } from '@/lib/i18n'

export function AddDeviceDialog({ onCreated }: { onCreated: (made: DeviceDetail) => void }) {
  const t = useT()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const create = useMutation({
    mutationFn: () => api.post<DeviceDetail>('/api/devices', { name, kind: 'device' }),
    onSuccess: (made) => {
      setOpen(false)
      setName('')
      void client.invalidateQueries({ queryKey: ['devices'] })
      onCreated(made)
    },
  })
  const submit = (e: FormEvent) => {
    e.preventDefault()
    create.mutate()
  }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild>
        <Button><Plus className="size-4" />{t('Add device')}</Button>
      </DialogTrigger>
      <DialogContent>
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Add a device')}</DialogTitle>
            <DialogDescription>
              {t("The name is the device's folder on the master (files/<name>/) and cannot change later.")}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="device-name">{t('Name')}</Label>
            <Input id="device-name" autoFocus placeholder="laptop" value={name} onChange={(e) => setName(e.target.value)} />
            <p className="text-xs text-muted-foreground">{t('Lowercase letters, digits, dot, underscore and dash; up to 63 characters.')}</p>
            {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          </div>
          <DialogFooter>
            <Button type="submit" disabled={!name.trim() || create.isPending}>{t('Create')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function Row({ d, now }: { d: DeviceSummary; now: number }) {
  const t = useT()
  return (
    <TableRow>
      <TableCell>
        <div className="flex items-center gap-2">
          <Link to={d.kind === 'device' ? `/devices/${d.name}` : `/files?device=${d.name}`} className="font-medium hover:underline">{d.name}</Link>
          <KindBadge kind={d.kind} />
        </div>
        {d.activity && <div className="mt-2 max-w-sm"><ActivityView activity={d.activity} /></div>}
      </TableCell>
      <TableCell>
        {d.kind === 'device' ? (
          <StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? t('Revoked') : d.online ? t('Online') : t('Offline')} />
        ) : (
          <span className="text-muted-foreground">—</span>
        )}
      </TableCell>
      <TableCell className="hidden md:table-cell text-muted-foreground">{d.os ?? '—'}</TableCell>
      <TableCell className="text-right tabular-nums">{count(d.files)}</TableCell>
      <TableCell className="text-right tabular-nums">{bytes(d.bytes)}</TableCell>
      <TableCell className="hidden sm:table-cell text-muted-foreground">{d.kind === 'device' ? ago(d.last_seen, now) : ago(d.last_collected, now)}</TableCell>
    </TableRow>
  )
}

export default function Devices() {
  const t = useT()
  const now = useNow()
  const [made, setMade] = useState<DeviceDetail | null>(null)
  const { data, error } = useQuery({ queryKey: ['devices'], queryFn: () => api.get<DeviceSummary[]>('/api/devices') })
  const shown = data?.filter((d) => d.kind !== 'source')

  return (
    <div>
      <PageHeader
        title={t('Devices')}
        description={t('Computers and phones that send their folders here, and local folders that live on the master itself.')}
        actions={<AddDeviceDialog onCreated={setMade} />}
      />
      {error ? <ErrorNote error={error} /> : !shown ? (
        <Skeleton className="h-48" />
      ) : shown.length === 0 ? (
        <Empty title={t('No devices yet')}>{t('Add a device to get a token and the command that connects it.')}</Empty>
      ) : (
        <div className="rounded-lg border bg-card">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('Name')}</TableHead>
                <TableHead>{t('Status')}</TableHead>
                <TableHead className="hidden md:table-cell">{t('System')}</TableHead>
                <TableHead className="text-right">{t('Files')}</TableHead>
                <TableHead className="text-right">{t('Size')}</TableHead>
                <TableHead className="hidden sm:table-cell">{t('Last seen')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>{shown.map((d) => <Row key={d.name} d={d} now={now} />)}</TableBody>
          </Table>
        </div>
      )}
      <EnrollDialog name={made?.device.name ?? ''} token={made?.token ?? null} onClose={() => setMade(null)} />
    </div>
  )
}
