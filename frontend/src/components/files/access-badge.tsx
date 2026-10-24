import { Lock, Pencil } from 'lucide-react'
import type { Entry } from '@/lib/types'
import { useT } from '@/lib/i18n'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

/** Whether the web app can change the files of a device: changeable, or read-only (with the reason on hover). */
export function AccessBadge({ writable, kind }: { writable: boolean; kind: Entry['device_kind'] }) {
  const t = useT()
  const why = writable
    ? t('You can upload, delete and restore files here.')
    : kind === 'device'
      ? t('This computer sends its own files: nothing can be uploaded or edited here. You can browse and download them, and hide them by deleting.')
      : kind === 'source'
        ? t('A source fills this folder: nothing can be uploaded or edited here. You can browse and download its files, and hide them by deleting.')
        : t('Your account can only read here.')
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge variant="outline" className={writable ? 'border-success/50 text-success' : 'text-muted-foreground'}>
          {writable ? <Pencil /> : <Lock />}
          {writable ? t('Changeable') : t('Read-only')}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>{why}</TooltipContent>
    </Tooltip>
  )
}
