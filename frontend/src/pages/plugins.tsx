import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Download, Trash2 } from 'lucide-react'
import { useEffect } from 'react'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { msg, useT } from '@/lib/i18n'
import { useSession } from '@/lib/session'
import type { PluginState } from '@/lib/types'

/** What the plugins the project makes are for. */
const ABOUT: Record<string, string> = {
  video: msg('Plays videos in the browser, and reads their details and pictures.'),
}

const STATE: Record<PluginState['state'], { label: string; variant: 'default' | 'secondary' | 'outline' | 'destructive' }> = {
  up: { label: msg('Running'), variant: 'default' },
  starting: { label: msg('Starting'), variant: 'secondary' },
  broken: { label: msg('Cannot run'), variant: 'destructive' },
  external: { label: msg('Set up by address'), variant: 'outline' },
}

export default function Plugins() {
  const t = useT()
  const client = useQueryClient()
  const session = useSession()
  const installed = useQuery({
    queryKey: ['plugins'],
    queryFn: () => api.get<{ installed: PluginState[] }>('/api/plugins'),
    refetchInterval: (q) => (q.state.data?.installed.some((p) => p.state === 'starting') ? 2000 : 15000),
  })
  // Goes to the internet, so it is asked for once and not again until the page is opened again.
  const offered = useQuery({ queryKey: ['plugins', 'available'], queryFn: () => api.get<{ id: string; version: string }[]>('/api/plugins/available'), staleTime: Infinity, retry: false })
  const refresh = () => client.invalidateQueries({ queryKey: ['plugins'] })
  const install = useMutation({
    mutationFn: (id: string) => api.post(`/api/plugins/${id}`),
    onSuccess: () => {
      toast.success(t('Installed. It starts in a few seconds.'))
      void refresh()
    },
    onError: (e) => toast.error(e.message),
  })
  const remove = useMutation({
    mutationFn: (id: string) => api.delete(`/api/plugins/${id}`),
    onSuccess: () => void refresh(),
    onError: (e) => toast.error(e.message),
  })

  // The web app learns of a module that came up or went away from the session, so ask for it again when the two differ.
  const known = Object.keys(session?.modules ?? {}).sort().join()
  const running = (installed.data?.installed ?? []).filter((p) => p.state === 'up').map((p) => p.id).sort().join()
  useEffect(() => {
    if (installed.data && known !== running) void client.invalidateQueries({ queryKey: ['session'] })
  }, [client, installed.data, known, running])

  const have = new Map((installed.data?.installed ?? []).map((p) => [p.id, p]))
  const ids = [...new Set([...have.keys(), ...(offered.data ?? []).map((o) => o.id)])].sort()
  const newest = new Map((offered.data ?? []).map((o) => [o.id, o.version]))
  const busy = install.isPending || remove.isPending

  return (
    <div className="space-y-6">
      <PageHeader
        title={t('Plugins')}
        description={t('Parts of fafnirx that are installed when you want them, so the server stays small. Each runs next to the server and can be removed again.')}
      />
      {installed.isPending ? (
        <Skeleton className="h-40" />
      ) : installed.error ? (
        <ErrorNote error={installed.error} />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t('Plugin')}</TableHead>
                <TableHead>{t('Version')}</TableHead>
                <TableHead>{t('State')}</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {ids.map((id) => {
                const plugin = have.get(id)
                const latest = newest.get(id)
                const state = plugin && STATE[plugin.state]
                return (
                  <TableRow key={id}>
                    <TableCell>
                      <p className="font-medium">{id}</p>
                      {ABOUT[id] && <p className="text-xs text-muted-foreground">{t(ABOUT[id])}</p>}
                      {plugin?.problem && <p className="text-xs text-destructive">{plugin.problem}</p>}
                    </TableCell>
                    <TableCell className="text-muted-foreground">{plugin?.version ?? latest}</TableCell>
                    <TableCell>{state ? <Badge variant={state.variant}>{t(state.label)}</Badge> : <span className="text-muted-foreground">{t('Not installed')}</span>}</TableCell>
                    <TableCell className="text-end">
                      <div className="flex justify-end gap-2">
                        {latest && plugin?.state !== 'external' && (!plugin || plugin.version !== latest) && (
                          <Button size="sm" disabled={busy} onClick={() => install.mutate(id)}>
                            <Download className="size-4" />
                            {plugin ? t('Update to {version}', { version: latest }) : t('Install')}
                          </Button>
                        )}
                        {plugin && plugin.state !== 'external' && (
                          <Button size="sm" variant="outline" disabled={busy} onClick={() => remove.mutate(id)}>
                            <Trash2 className="size-4" />
                            {t('Remove')}
                          </Button>
                        )}
                      </div>
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
          {offered.error && <p className="text-sm text-muted-foreground">{t('The plugins that can be installed could not be listed: {error}', { error: offered.error.message })}</p>}
        </>
      )}
    </div>
  )
}
