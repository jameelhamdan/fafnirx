import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useLocation, useNavigate, useParams } from 'react-router-dom'
import { ErrorNote } from '@/components/common'
import { Viewer } from '@/components/library/viewer'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api, query } from '@/lib/api'
import type { Asset } from '@/lib/types'
import { backgroundOf, useCurrentSource } from '@/lib/viewer-source'
import { useT } from '@/lib/i18n'

const parent = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf('/')))

/**
 * A photo or video at an address of its own (`/watch/<id>`), which can be sent to whoever may see the file. Opened from the
 * timeline, an album or the file browser it is shown over that page and steps through what the page shows; opened on its own
 * (a link, a reload) it is a page of its own and steps through the files of the folder it is in.
 */
export default function Watch() {
  const t = useT()
  const id = useParams().id ?? ''
  const navigate = useNavigate()
  const location = useLocation()
  const over = backgroundOf(location) !== undefined
  const source = useCurrentSource()
  const given = source?.assets.some((a) => a.id === id) ? source : null
  // The folder is listed once, for the file the page was opened on; moving to the next file only changes the address.
  const [opened] = useState(id)
  const { data, error } = useQuery({
    queryKey: ['folder', opened],
    queryFn: () => api.get<{ assets: Asset[] }>(`/api/library/${opened}/folder`),
    enabled: given === null,
  })
  // Moving on replaces the address and keeps the page under the viewer.
  const step = (next: string) => navigate(`/watch/${next}`, { replace: true, state: location.state })
  if (given) {
    const close = () => (window.history.state?.idx > 0 ? navigate(-1) : navigate(backgroundOf(location)?.pathname ?? '/', { replace: true }))
    return <Viewer assets={given.assets} id={id} onChange={step} onClose={close} onEdge={given.onEdge} />
  }
  if (error) {
    return (
      <div className="space-y-3">
        <ErrorNote error={error} />
        <p className="text-sm text-muted-foreground">{t('This link is not valid any more, or the file is gone.')}</p>
        <Button variant="outline" asChild><Link to="/files">{t('Back to Files')}</Link></Button>
      </div>
    )
  }
  if (!data) return <Skeleton className="h-96" />
  const here = data.assets.find((a) => a.id === opened) ?? data.assets[0]
  const close = () => {
    if (over) navigate(-1)
    else if (window.history.state?.idx > 0) navigate(-1)
    else navigate(`/files${query({ device: here.device, path: parent(here.path), file: here.path })}`, { replace: true })
  }
  return (
    <>
      <Link to="/files" className="sr-only">{t('Files')}</Link>
      <Viewer assets={data.assets} id={id} onChange={step} onClose={close} />
    </>
  )
}
