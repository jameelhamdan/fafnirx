import { FolderOpen, Images } from 'lucide-react'
import { Link } from 'react-router-dom'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Thumb } from '@/components/library/thumb'
import { Skeleton } from '@/components/ui/skeleton'
import { mediaUrl } from '@/lib/api'
import { bytes, plural } from '@/lib/format'
import { useAlbums } from '@/lib/library'
import { useT } from '@/lib/i18n'

export default function Albums() {
  const t = useT()
  const albums = useAlbums()
  return (
    <div>
      <PageHeader
        title={t('Albums')}
        description={t('Shortcuts to folders. What is in the folder is in the album, and it follows the folder when that is renamed or moved. Deleting an album never deletes the files.')}
      />
      {albums.error ? <ErrorNote error={albums.error} /> : !albums.data ? (
        <Skeleton className="h-64" />
      ) : albums.data.length === 0 ? (
        <Empty title={t('No albums yet')}>{t('In Files, open a folder and choose Show as album.')}</Empty>
      ) : (
        <ul className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {albums.data.map((a) => (
            <li key={a.id}>
              <Link to={`/albums/${a.id}`} className="group block overflow-hidden rounded-md border bg-card transition-colors hover:border-foreground/30 focus-visible:ring-2 focus-visible:ring-ring">
                <div className="relative aspect-square bg-muted">
                  {a.cover ? (
                    <Thumb src={mediaUrl(a.cover, 'thumb')} className="size-full transition-transform group-hover:scale-105" fallback={<Images className="size-8 text-muted-foreground" />} />
                  ) : (
                    <div className="grid size-full place-items-center"><Images className="size-8 text-muted-foreground" /></div>
                  )}
                  <span title={t('Shows a folder')} className="absolute top-1.5 start-1.5 grid size-6 place-items-center rounded-full bg-media-ground/55 text-on-media">
                    <FolderOpen className="size-3.5" />
                  </span>
                </div>
                <div className="p-3">
                  <p className="truncate font-medium">{a.name}</p>
                  <p className="text-xs text-muted-foreground">{plural(a.count, 'item')} · {bytes(a.bytes)}</p>
                </div>
              </Link>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
