import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FolderOpen, KeyRound, Ban, Unplug } from 'lucide-react'
import { useState } from 'react'
import { Link, useParams } from 'react-router-dom'
import { toast } from 'sonner'
import { ActivityView, ErrorNote, Fact, KindBadge, PageHeader, StatusDot } from '@/components/common'
import { EnrollDialog } from '@/components/enroll-dialog'
import { Confirm } from '@/components/confirm'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, bytes, count, dateTime } from '@/lib/format'
import type { DeviceDetail } from '@/lib/types'
import { cn } from '@/lib/utils'
import { useT } from '@/lib/i18n'
import { deviceAddress } from '@/lib/session'

export default function DeviceDetailPage() {
  const t = useT()
  const { name = '' } = useParams()
  const client = useQueryClient()
  const now = useNow()
  const [token, setToken] = useState<string | null>(null)
  const [confirm, setConfirm] = useState<'revoke' | 'rotate' | 'release' | null>(null)
  const { data, error } = useQuery({ queryKey: ['device', name], queryFn: () => api.get<DeviceDetail>(`/api/devices/${deviceAddress(name)}`) })
  const refresh = () => client.invalidateQueries({ queryKey: ['device', name] })

  const rotate = useMutation({
    mutationFn: () => api.post<DeviceDetail>(`/api/devices/${deviceAddress(name)}/token`),
    onSuccess: (made) => { setToken(made.token); void refresh() },
    onError: (e) => toast.error(e.message),
  })
  const release = useMutation({
    mutationFn: () => api.post(`/api/devices/${deviceAddress(name)}/release`),
    onSuccess: () => { toast.success(t('{name} released. The next computer that connects with its token takes it over.', { name })); void refresh() },
    onError: (e) => toast.error(e.message),
  })
  const revoke = useMutation({
    mutationFn: () => api.post(`/api/devices/${deviceAddress(name)}/revoke`),
    onSuccess: () => { toast.success(t('{name} revoked. Its data is kept.', { name })); void refresh() },
    onError: (e) => toast.error(e.message),
  })

  if (error) return <ErrorNote error={error} />
  if (!data) return <Skeleton className="h-64" />
  const d = data.device

  return (
    <div>
      <PageHeader
        title={d.folder}
        description={<span className="flex items-center gap-3"><KindBadge kind={d.kind} />{d.kind === 'device' && <StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? t('Revoked') : d.online ? t('Online') : t('Offline, seen {when}', { when: ago(d.last_seen, now) })} />}</span>}
        actions={
          <>
            <Button variant="outline" asChild><Link to={`/files?device=${encodeURIComponent(d.folder)}`}><FolderOpen className="size-4" />{t('Browse files')}</Link></Button>
            {d.kind === 'device' && (
              <>
                <Button variant="outline" onClick={() => setConfirm('rotate')}><KeyRound className="size-4" />{d.revoked ? t('New token (re-enables)') : t('Rotate token')}</Button>
                {!d.revoked && <Button variant="outline" onClick={() => setConfirm('release')}><Unplug className="size-4" />{t('Release')}</Button>}
                {!d.revoked && <Button variant="outline" onClick={() => setConfirm('revoke')}><Ban className="size-4" />{t('Revoke')}</Button>}
              </>
            )}
          </>
        }
      />

      {d.activity && (
        <Card className="mb-6">
          <CardHeader><CardTitle>{t('Working now')}</CardTitle></CardHeader>
          <CardContent><ActivityView activity={d.activity} /></CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-3">
        <Card>
          <CardHeader><CardTitle>{t('Details')}</CardTitle></CardHeader>
          <CardContent>
            <dl className="grid grid-cols-2 gap-4">
              <Fact label={t('Files')}>{count(d.files)}</Fact>
              <Fact label={t('Stored')}>{bytes(d.bytes)}</Fact>
              <Fact label={t('Host')}>{d.hostname}</Fact>
              <Fact label={t('System')}>{d.os}</Fact>
              <Fact label={t('Address')}>{d.ip}</Fact>
              <Fact label={t('Client')}>{d.client_version}</Fact>
              <Fact label={t('Disk free')}>{d.disk_total ? t('{free} of {total}', { free: bytes(d.disk_free), total: bytes(d.disk_total) }) : null}</Fact>
              <Fact label={t('Last collected')}>{d.last_collected ? ago(d.last_collected, now) : null}</Fact>
              <Fact label={t('Added')}>{dateTime(d.created_at)}</Fact>
            </dl>
          </CardContent>
        </Card>
        <Card className="lg:col-span-2">
          <CardHeader><CardTitle>{t('Recent changes')}</CardTitle></CardHeader>
          <CardContent>
            {data.events.length === 0 ? <p className="text-sm text-muted-foreground">{t('Nothing collected yet.')}</p> : (
              <ul className="divide-y">
                {data.events.map((e) => (
                  <li key={e.id} className="flex items-center gap-3 py-2 text-sm">
                    <span className={cn('w-16 shrink-0 text-xs font-medium uppercase', e.action === 'delete' ? 'text-destructive' : e.action === 'update' ? 'text-warning' : e.action === 'rename' ? 'text-foreground' : 'text-success')}>{e.action}</span>
                    <Link to={`/files?device=${encodeURIComponent(d.folder)}&file=${encodeURIComponent(e.path)}`} className="min-w-0 flex-1 truncate hover:underline" title={e.path}>{e.path}</Link>
                    <span className="shrink-0 text-xs text-muted-foreground">{ago(e.ts, now)}</span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </div>

      <Confirm
        open={confirm === 'rotate'}
        title={t('Rotate the token?')}
        description={t('The old token stops working at once. The device must be enrolled again with the new one.')}
        action={t('Rotate')}
        onCancel={() => setConfirm(null)}
        onConfirm={() => { setConfirm(null); rotate.mutate() }}
      />
      <Confirm
        open={confirm === 'release'}
        title={t('Release {name}?', { name: d.name })}
        description={t('A device is used by one computer at a time, and a second one using its token is refused. Release it to let the next computer that connects take it over, for example after replacing the computer. The computer that has it now is refused from then on.')}
        action={t('Release')}
        onCancel={() => setConfirm(null)}
        onConfirm={() => { setConfirm(null); release.mutate() }}
      />
      <Confirm
        open={confirm === 'revoke'}
        title={t('Revoke {name}?', { name: d.name })}
        description={t('It can no longer send data. What it already sent stays on the master.')}
        action={t('Revoke')}
        destructive
        onCancel={() => setConfirm(null)}
        onConfirm={() => { setConfirm(null); revoke.mutate() }}
      />
      <EnrollDialog name={d.name} token={token} onClose={() => setToken(null)} />
    </div>
  )
}
