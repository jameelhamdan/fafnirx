import { Link } from 'react-router-dom'
import { useNow } from '@/hooks/use-now'
import { ago } from '@/lib/format'
import { msg, useT } from '@/lib/i18n'
import { filesLink } from '@/lib/links'
import type { Recent } from '@/lib/types'
import { cn } from '@/lib/utils'

const ACTION: Record<string, string> = { add: msg('add'), update: msg('update'), delete: msg('delete'), rename: msg('rename') }
const COLOUR: Record<string, string> = { delete: 'text-destructive', update: 'text-warning', rename: 'text-foreground' }

/** Recent changes to files, newest first, each linking to the file; `showFolder` names the folder in front of the path. */
export function EventList({ events, empty, showFolder = false, folder }: { events: Recent[]; empty: string; showFolder?: boolean; folder?: string }) {
  const t = useT()
  const now = useNow()
  if (events.length === 0) return <p className="text-sm text-muted-foreground">{empty}</p>
  return (
    <ul className="divide-y">
      {events.map((e) => (
        <li key={e.id} className="flex items-baseline gap-2 py-1 text-[13px]">
          <span className={cn('w-16 shrink-0 text-xs font-medium', COLOUR[e.action] ?? 'text-success')}>{t(ACTION[e.action] ?? e.action)}</span>
          <Link to={filesLink(folder ?? e.device, '', e.path)} className="num min-w-0 flex-1 text-[12px] [overflow-wrap:anywhere] hover:underline" title={e.path}>
            {showFolder && <span className="text-muted-foreground">{e.device}/</span>}{e.path}
          </Link>
          <span className="num shrink-0 text-[11px] text-muted-foreground">{ago(e.ts, now)}</span>
        </li>
      ))}
    </ul>
  )
}
