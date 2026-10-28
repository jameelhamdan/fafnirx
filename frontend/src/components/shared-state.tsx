import { Link } from 'react-router-dom'
import { Badge } from '@/components/ui/badge'
import { useNow } from '@/hooks/use-now'
import { ago, bytes, count } from '@/lib/format'
import { useT } from '@/lib/i18n'
import type { SharedState } from '@/lib/types'

/** What computers say about their shared folders: one line per folder (`folders`: each name links to its files) or per computer. */
export function SharedStateList({ rows, folders = false }: { rows: SharedState[]; folders?: boolean }) {
  const t = useT()
  const now = useNow()
  return (
    <ul className="divide-y">
      {rows.map((r) => {
        const trouble = r.last_error !== '' || r.failing > 0
        const note = [r.held > 0 && t('{count} deletions held back', { count: r.held }), r.failing > 0 && t('{count} files waiting for a retry', { count: r.failing })].filter(Boolean).join(' · ')
        return (
          <li key={r.name} className="space-y-1 py-2 text-sm">
            <div className="flex flex-wrap items-center gap-2">
              {folders ? <Link to={`/files?device=${encodeURIComponent(r.name)}`} className="font-medium hover:underline">{r.name}</Link> : <span className="font-medium">{r.name}</span>}
              {r.last_cycle === 0 ? <Badge variant="outline">{t('Not synced yet')}</Badge> : trouble ? <Badge variant="destructive">{t('Needs attention')}</Badge> : <Badge variant="secondary">{t('In step')}</Badge>}
              <span className="text-muted-foreground">{t('{files} files, {size}', { files: count(r.files), size: bytes(r.bytes) })}</span>
              {r.last_cycle > 0 && <span className="text-muted-foreground">{t('Synced {when}', { when: ago(r.last_cycle, now) })}</span>}
            </div>
            {note && <p className="text-xs text-muted-foreground">{note}</p>}
            {r.last_error && <p className="text-xs text-destructive">{r.last_error}</p>}
          </li>
        )
      })}
    </ul>
  )
}
