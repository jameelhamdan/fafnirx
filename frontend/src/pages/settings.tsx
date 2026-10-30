import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { RescanButton } from '@/components/files/rescan-button'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { ChangePassword } from '@/components/change-password'
import { api } from '@/lib/api'
import { useRole, useSession } from '@/lib/session'
import type { PluginState, Settings as SettingsData } from '@/lib/types'
import { msg, useT } from '@/lib/i18n'

function SettingsForm({ data }: { data: SettingsData }) {
  const t = useT()
  const client = useQueryClient()
  const [versions, setVersions] = useState(String(data.version_count))
  const [days, setDays] = useState(String(data.retention_days))
  const [thumbnails, setThumbnails] = useState(data.thumbnails)
  const save = useMutation({
    mutationFn: () => api.put<SettingsData>('/api/settings', { version_count: Number(versions), retention_days: Number(days), thumbnails }),
    onSuccess: (saved) => { client.setQueryData(['settings'], saved); toast.success(t('Settings saved')) },
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => { e.preventDefault(); save.mutate() }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('Versions and retention')}</CardTitle>
        <CardDescription>
          {t('When a file changes, the old content is kept as a version. A deleted file, and versions beyond the count, are removed for good once the retention period has passed.')}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="versions">{t('Versions kept per file')}</Label>
            <Input id="versions" type="number" min="1" max="1000" value={versions} onChange={(e) => setVersions(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="days">{t('Retention (days)')}</Label>
            <Input id="days" type="number" min="1" max="3650" value={days} onChange={(e) => setDays(e.target.value)} />
          </div>
          <div className="flex items-start justify-between gap-4 rounded-lg border p-3">
            <div className="space-y-1">
              <Label htmlFor="thumbnails">{t('Thumbnails and previews')}</Label>
              <p className="text-sm text-muted-foreground">
                {t('Made when someone looks at a photo or video, and kept. Off: none is made, not even on demand, and an icon is shown instead.')}
              </p>
            </div>
            <Switch id="thumbnails" checked={thumbnails} onCheckedChange={setThumbnails} />
          </div>
          <Button type="submit" disabled={save.isPending}>{t('Save')}</Button>
        </form>
      </CardContent>
    </Card>
  )
}

/** Everybody changes their own password; the rest of the page is for administrators. */
function Account() {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('Password')}</CardTitle>
        <CardDescription>{t('Changing it ends your other logins.')}</CardDescription>
      </CardHeader>
      <CardContent>
        <Button variant="outline" onClick={() => setOpen(true)}>{t('Change password')}</Button>
        <ChangePassword open={open} onClose={() => setOpen(false)} />
      </CardContent>
    </Card>
  )
}

const PLUGIN_STATE: Record<PluginState['state'], { label: string; variant: 'default' | 'secondary' | 'outline' | 'destructive' }> = {
  up: { label: msg('Running'), variant: 'default' },
  setup: { label: msg('Running, not set up'), variant: 'destructive' },
  starting: { label: msg('Starting'), variant: 'secondary' },
  broken: { label: msg('Not running'), variant: 'destructive' },
  external: { label: msg('Set up by address'), variant: 'outline' },
}

/** What runs as a module. Read only: the server administrator installs and removes plugins (FAFNIRX_PLUGINS, docs/modules.md). */
function Plugins() {
  const t = useT()
  const client = useQueryClient()
  const session = useSession()
  const { data, error } = useQuery({
    queryKey: ['plugins'],
    queryFn: () => api.get<{ installed: PluginState[] }>('/api/plugins'),
    refetchInterval: (q) => (q.state.data?.installed.some((p) => p.state === 'starting') ? 2000 : 15000),
  })
  // The web app learns of a module that came up or went away from the session, so ask for it again when the two differ.
  const known = Object.keys(session?.modules ?? {}).sort().join()
  const running = (data?.installed ?? []).filter((p) => p.state === 'up' || p.state === 'setup').map((p) => p.id).sort().join()
  useEffect(() => {
    if (data && known !== running) void client.invalidateQueries({ queryKey: ['session'] })
  }, [client, data, known, running])

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('Plugins')}</CardTitle>
        <CardDescription>{t('Optional parts that run next to the server. The server administrator installs and removes them: set FAFNIRX_PLUGINS on the server.')}</CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <ErrorNote error={error} />
        ) : !data ? (
          <Skeleton className="h-12" />
        ) : data.installed.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t('None installed.')}</p>
        ) : (
          <ul className="divide-y">
            {data.installed.map((p) => (
              <li key={p.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2">
                <div className="min-w-0 flex-1">
                  <p className="font-medium">{p.id} <span className="text-sm font-normal text-muted-foreground">{p.version}</span></p>
                  {p.problem && <p className="text-xs text-destructive">{p.problem}</p>}
                </div>
                <Badge variant={PLUGIN_STATE[p.state].variant}>{t(PLUGIN_STATE[p.state].label)}</Badge>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  )
}

function Administration() {
  const t = useT()
  const { data, error } = useQuery({ queryKey: ['settings'], queryFn: () => api.get<SettingsData>('/api/settings') })
  return (
    <>
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-56" /> : <SettingsForm data={data} />}
      <Card>
        <CardHeader>
          <CardTitle>{t('File store')}</CardTitle>
          <CardDescription>
            {t('Files put on the disk by hand, or over a network share, are noticed on their own within a few minutes. Scan now to look right away.')}
          </CardDescription>
        </CardHeader>
        <CardContent><RescanButton /></CardContent>
      </Card>
      <Plugins />
    </>
  )
}

export default function Settings() {
  const t = useT()
  const admin = useRole() === 'admin'
  return (
    <div className="space-y-6">
      <PageHeader title={t('Settings')} />
      <div className="grid items-start gap-6 lg:grid-cols-2">
        <Account />
        {admin && <Administration />}
      </div>
    </div>
  )
}
