import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { ErrorNote } from '@/components/common'
import { Viewer } from '@/components/library/viewer'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { api, query } from '@/lib/api'
import type { Asset } from '@/lib/types'
import { useT } from '@/lib/i18n'

const parent = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf('/')))

/**
 * A photo or video on a page of its own, opened from the file browser (`/watch/<id>`): the same viewer as in the
 * library and the albums, stepping through the files of the folder it is in. The address can be shared with whoever can
 * see the file.
 */
export default function Watch() {
  const t = useT()
  const id = useParams().id ?? ''
  const navigate = useNavigate()
  // The folder is listed once, for the file the page was opened on; moving to the next file only changes the address.
  const [opened] = useState(id)
  const { data, error } = useQuery({
    queryKey: ['folder', opened],
    queryFn: () => api.get<{ assets: Asset[] }>(`/api/library/${opened}/folder`),
  })
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
    if (window.history.state?.idx > 0) navigate(-1)
    else navigate(`/files${query({ device: here.device, path: parent(here.path), file: here.path })}`, { replace: true })
  }
  return (
    <>
      <Link to="/files" className="sr-only">{t('Files')}</Link>
      <Viewer assets={data.assets} id={id} onChange={(next) => navigate(`/watch/${next}`, { replace: true })} onClose={close} />
    </>
  )
}
