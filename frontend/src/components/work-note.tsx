import { AlertTriangle, Loader2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useT } from '@/lib/i18n'

/** Says that a photo or video is not ready: its details and pictures are queued, being made, or failed. Nothing when done. */
export function WorkNote({ work }: { work: 'waiting' | 'running' | 'failed' | null | undefined }) {
  const t = useT()
  if (!work) return null
  if (work === 'failed') {
    return (
      <p role="status" className="flex items-start gap-2 rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
        <span>{t('Processing failed.')} <Link to="/queue" className="underline">{t('See what went wrong')}</Link></span>
      </p>
    )
  }
  return (
    <p role="status" className="flex items-start gap-2 rounded-md border bg-muted/50 p-3 text-sm">
      <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin" />
      <span>{work === 'running' ? t('Being processed now: its details and pictures are being made.') : t('Waiting in the queue: its details and pictures are not ready yet.')} <Link to="/queue" className="underline">{t('Processing')}</Link></span>
    </p>
  )
}
