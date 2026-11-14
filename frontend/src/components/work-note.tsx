import { AlertTriangle, Loader2 } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useT } from '@/lib/i18n'
import { Callout } from './common'

/** Says that a photo or video is not ready: its details and pictures are queued, being made, or failed. Nothing when done. */
export function WorkNote({ work }: { work: 'waiting' | 'running' | 'failed' | null | undefined }) {
  const t = useT()
  if (!work) return null
  if (work === 'failed') {
    return (
      <Callout tone="danger" role="status" className="flex items-start gap-2 text-foreground">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" />
        <span>{t('Processing failed.')} <Link to="/queue" className="underline">{t('See what went wrong')}</Link></span>
      </Callout>
    )
  }
  return (
    <Callout role="status" className="flex items-start gap-2 border-border bg-muted/50">
      <Loader2 className="mt-0.5 size-4 shrink-0 animate-spin" />
      <span>{work === 'running' ? t('Being processed now: its details and pictures are being made.') : t('Waiting in the queue: its details and pictures are not ready yet.')} <Link to="/queue" className="underline">{t('Processing')}</Link></span>
    </Callout>
  )
}
