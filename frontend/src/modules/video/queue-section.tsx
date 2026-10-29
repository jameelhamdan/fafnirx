import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { ago, msg, useNow, useT } from '@/sdk'

/** What the module reports (its `GET /status`). */
interface Report {
  slots: number
  made: number
  active: { kind: string; path: string; running: boolean; since: number }[]
  failed: { kind: string; path: string; error: string; at: number }[]
}

const KIND: Record<string, string> = {
  segment: msg('Video segments'),
  subtitles: msg('Subtitles'),
}

/** The video module's own work: segments being cut for a video that is playing, subtitle tracks being converted. */
export default function QueueSection({ report }: { report: unknown }) {
  const t = useT()
  const now = useNow()
  const { slots, made, active, failed } = report as Report
  return (
    <Card>
      <CardHeader><CardTitle>{t('Streaming')}</CardTitle></CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          {t('Made as videos are watched, {slots} at a time. {made} made since it started.', { slots, made })}
        </p>
        {active.length === 0 && failed.length === 0 && <p className="text-sm text-muted-foreground">{t('Nothing is being made now.')}</p>}
        <ul className="divide-y text-sm">
          {active.map((a, i) => (
            <li key={i} className="flex items-center justify-between gap-3 py-1.5">
              <span className="min-w-0 truncate"><Badge variant="secondary" className="mr-2">{t(KIND[a.kind] ?? a.kind)}</Badge>{a.path}</span>
              <span className="shrink-0 text-muted-foreground">{a.running ? t('Running') : t('Waiting')} · {ago(a.since, now)}</span>
            </li>
          ))}
          {failed.map((f, i) => (
            <li key={`f${i}`} className="py-1.5 text-destructive">
              <span className="mr-2 font-medium">{t(KIND[f.kind] ?? f.kind)}</span>{f.path}
              <span className="block truncate text-xs" title={f.error}>{f.error} · {ago(f.at, now)}</span>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  )
}
