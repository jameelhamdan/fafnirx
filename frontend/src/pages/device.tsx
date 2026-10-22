import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { FolderOpen, KeyRound, Ban } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { Link, useParams } from 'react-router-dom'
import { toast } from 'sonner'
import { ActivityView, ErrorNote, KindBadge, PageHeader, StatusDot } from '@/components/common'
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

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="mt-0.5 text-sm">{children || '—'}</dd>
    </div>
  )
}

export default function DeviceDetailPage() {
  const { name = '' } = useParams()
  const client = useQueryClient()
  const now = useNow()
  const [token, setToken] = useState<string | null>(null)
  const [confirm, setConfirm] = useState<'revoke' | 'rotate' | null>(null)
  const { data, error } = useQuery({ queryKey: ['device', name], queryFn: () => api.get<DeviceDetail>(`/api/devices/${name}`) })
  const refresh = () => client.invalidateQueries({ queryKey: ['device', name] })

  const rotate = useMutation({
    mutationFn: () => api.post<DeviceDetail>(`/api/devices/${name}/token`),
    onSuccess: (made) => { setToken(made.token); void refresh() },
    onError: (e) => toast.error(e.message),
  })
  const revoke = useMutation({
    mutationFn: () => api.post(`/api/devices/${name}/revoke`),
    onSuccess: () => { toast.success(`${name} revoked. Its data is kept.`); void refresh() },
    onError: (e) => toast.error(e.message),
  })

  if (error) return <ErrorNote error={error} />
  if (!data) return <Skeleton className="h-64" />
  const d = data.device

  return (
    <div>
      <PageHeader
        title={d.name}
        description={<span className="flex items-center gap-3"><KindBadge kind={d.kind} />{d.kind === 'device' && <StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? 'Revoked' : d.online ? 'Online' : `Offline, seen ${ago(d.last_seen, now)}`} />}</span>}
        actions={
          <>
            <Button variant="outline" asChild><Link to={`/files?device=${d.name}`}><FolderOpen className="size-4" />Browse files</Link></Button>
            {d.kind === 'device' && (
              <>
                <Button variant="outline" onClick={() => setConfirm('rotate')}><KeyRound className="size-4" />{d.revoked ? 'New token (re-enables)' : 'Rotate token'}</Button>
                {!d.revoked && <Button variant="outline" onClick={() => setConfirm('revoke')}><Ban className="size-4" />Revoke</Button>}
              </>
            )}
          </>
        }
      />

      {d.activity && (
        <Card className="mb-6">
          <CardHeader><CardTitle>Working now</CardTitle></CardHeader>
          <CardContent><ActivityView activity={d.activity} /></CardContent>
        </Card>
      )}

      <div className="grid gap-6 lg:grid-cols-3">
        <Card>
          <CardHeader><CardTitle>Details</CardTitle></CardHeader>
          <CardContent>
            <dl className="grid grid-cols-2 gap-4">
              <Fact label="Files">{count(d.files)}</Fact>
              <Fact label="Stored">{bytes(d.bytes)}</Fact>
              <Fact label="Host">{d.hostname}</Fact>
              <Fact label="System">{d.os}</Fact>
              <Fact label="Address">{d.ip}</Fact>
              <Fact label="Client">{d.client_version}</Fact>
              <Fact label="Disk free">{d.disk_total ? `${bytes(d.disk_free)} of ${bytes(d.disk_total)}` : null}</Fact>
              <Fact label="Last collected">{d.last_collected ? ago(d.last_collected, now) : null}</Fact>
              <Fact label="Added">{dateTime(d.created_at)}</Fact>
            </dl>
          </CardContent>
        </Card>
        <Card className="lg:col-span-2">
          <CardHeader><CardTitle>Recent changes</CardTitle></CardHeader>
          <CardContent>
            {data.events.length === 0 ? <p className="text-sm text-muted-foreground">Nothing collected yet.</p> : (
              <ul className="divide-y">
                {data.events.map((e) => (
                  <li key={e.id} className="flex items-center gap-3 py-2 text-sm">
                    <span className={cn('w-16 shrink-0 text-xs font-medium uppercase', e.action === 'delete' ? 'text-destructive' : e.action === 'update' ? 'text-warning' : 'text-success')}>{e.action}</span>
                    <Link to={`/files?device=${d.name}&file=${encodeURIComponent(e.path)}`} className="min-w-0 flex-1 truncate hover:underline" title={e.path}>{e.path}</Link>
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
        title="Rotate the token?"
        description="The old token stops working at once. The device must be enrolled again with the new one."
        action="Rotate"
        onCancel={() => setConfirm(null)}
        onConfirm={() => { setConfirm(null); rotate.mutate() }}
      />
      <Confirm
        open={confirm === 'revoke'}
        title={`Revoke ${d.name}?`}
        description="It can no longer send data. What it already sent stays on the master."
        action="Revoke"
        destructive
        onCancel={() => setConfirm(null)}
        onConfirm={() => { setConfirm(null); revoke.mutate() }}
      />
      <EnrollDialog name={d.name} token={token} onClose={() => setToken(null)} />
    </div>
  )
}
