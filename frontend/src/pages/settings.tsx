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
import { api } from '@/lib/api'
import { useSession } from '@/lib/session'
import type { PluginState, Settings as SettingsData } from '@/lib/types'
import { msg, useT } from '@/lib/i18n'

/** Saves some of the settings: the server takes all of them at once, so the rest are sent as they are. */
function useSaveSettings(data: SettingsData) {
  const t = useT()
  const client = useQueryClient()
  return useMutation({
    mutationFn: (changes: Partial<SettingsData>) => api.put<SettingsData>('/api/settings', { ...data, ...changes }),
    onSuccess: (saved) => {
      client.setQueryData(['settings'], saved)
      // The menu follows the session.
      void client.invalidateQueries({ queryKey: ['session'] })
      toast.success(t('Settings saved'))
    },
    onError: (e) => toast.error(e.message),
  })
}

function VersionsForm({ data }: { data: SettingsData }) {
  const t = useT()
  const [versions, setVersions] = useState(String(data.version_count))
  const [days, setDays] = useState(String(data.retention_days))
  const save = useSaveSettings(data)
  const submit = (e: FormEvent) => { e.preventDefault(); save.mutate({ version_count: Number(versions), retention_days: Number(days) }) }
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
          <Button type="submit" disabled={save.isPending}>{t('Save')}</Button>
        </form>
      </CardContent>
    </Card>
  )
}

function LibraryForm({ data }: { data: SettingsData }) {
  const t = useT()
  const [thumbnails, setThumbnails] = useState(data.thumbnails)
  const [photos, setPhotos] = useState(data.photos)
  const save = useSaveSettings(data)
  const submit = (e: FormEvent) => { e.preventDefault(); save.mutate({ thumbnails, photos }) }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('Photos and videos')}</CardTitle>
        <CardDescription>{t('What the web app makes and shows of the pictures and videos in the library.')}</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="space-y-4">
          <div className="flex items-start justify-between gap-4 rounded-lg border p-3">
            <div className="space-y-1">
              <Label htmlFor="thumbnails">{t('Thumbnails and previews')}</Label>
              <p className="text-sm text-muted-foreground">
                {t('Made when someone looks at a photo or video, and kept. Off: none is made, not even on demand, and an icon is shown instead.')}
              </p>
            </div>
            <Switch id="thumbnails" checked={thumbnails} onCheckedChange={setThumbnails} />
          </div>
          <div className="flex items-start justify-between gap-4 rounded-lg border p-3">
            <div className="space-y-1">
              <Label htmlFor="photos">{t('Photo pages')}</Label>
              <p className="text-sm text-muted-foreground">
                {t('Timeline, Favorites, Albums and Archive, for everybody. Off: they are hidden and their pages are closed. Photos are still read and shown in Files.')}
              </p>
            </div>
            <Switch id="photos" checked={photos} onCheckedChange={setPhotos} />
          </div>
          <Button type="submit" disabled={save.isPending}>{t('Save')}</Button>
        </form>
      </CardContent>
    </Card>
  )
}

const PLUGIN_STATE: Record<PluginState['state'], { label: string; variant: 'default' | 'secondary' | 'outline' | 'destructive' }> = {
  up: { label: msg('Running'), variant: 'default' },
  setup: { label: msg('Running, not set up'), variant: 'destructive' },
  down: { label: msg('Not reachable'), variant: 'destructive' },
}

/** The modules that have announced themselves to the master (docs/modules.md). Read only. */
function Plugins() {
  const t = useT()
  const client = useQueryClient()
  const session = useSession()
  const { data, error } = useQuery({
    queryKey: ['plugins'],
    queryFn: () => api.get<{ plugins: PluginState[] }>('/api/plugins'),
    refetchInterval: (q) => (q.state.data?.plugins.some((p) => p.state === 'down') ? 5000 : 15000),
  })
  // The web app learns of a module that came up or went away from the session, so ask for it again when the two differ.
  const known = Object.keys(session?.modules ?? {}).sort().join()
  const running = (data?.plugins ?? []).filter((p) => p.state === 'up' || p.state === 'setup').map((p) => p.id).sort().join()
  useEffect(() => {
    if (data && known !== running) void client.invalidateQueries({ queryKey: ['session'] })
  }, [client, data, known, running])

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('Plugins')}</CardTitle>
        <CardDescription>{t('Optional parts that run as containers next to the server. They connect on their own once the server administrator starts them.')}</CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <ErrorNote error={error} />
        ) : !data ? (
          <Skeleton className="h-12" />
        ) : data.plugins.length === 0 ? (
          <p className="text-sm text-muted-foreground">{t('None connected.')}</p>
        ) : (
          <ul className="divide-y">
            {data.plugins.map((p) => (
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

function FileStore() {
  const t = useT()
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t('File store')}</CardTitle>
        <CardDescription>
          {t('Files put on the disk by hand, or over a network share, are noticed on their own within a few minutes. Scan now to look right away.')}
        </CardDescription>
      </CardHeader>
      <CardContent><RescanButton /></CardContent>
    </Card>
  )
}

/** The settings of the whole application, for administrators. What belongs to one person (their password, logging out) is in the menu of their name. */
export default function Settings() {
  const t = useT()
  const { data, error } = useQuery({ queryKey: ['settings'], queryFn: () => api.get<SettingsData>('/api/settings') })
  // Two columns of cards that each take the height they need, so there are no gaps between them; on a narrow screen one column.
  const column = 'flex min-w-0 flex-1 flex-col gap-6'
  return (
    <div className="space-y-6">
      <PageHeader title={t('Settings')} description={t('For the whole application. Your password and logging out are in the menu of your name, top right.')} />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-56" /> : (
        <div className="flex flex-col gap-6 lg:flex-row lg:items-start">
          <div className={column}>
            <VersionsForm data={data} />
            <FileStore />
          </div>
          <div className={column}>
            <LibraryForm data={data} />
            <Plugins />
          </div>
        </div>
      )}
    </div>
  )
}
