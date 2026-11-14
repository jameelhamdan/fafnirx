import { TriangleAlert } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useI18n } from '@/lib/i18n'
import { usePlugins } from '@/lib/plugins'

/** A plugin the server administrator said must run (`FAFNIRX_PLUGINS`) but that is not working. Shown on every page to administrators. */
export function PluginAlert() {
  const { t } = useI18n()
  const { data } = usePlugins()
  const broken = (data?.plugins ?? []).filter((p) => p.expected && p.state !== 'up')
  if (broken.length === 0) return null
  return (
    <div role="alert" className="mb-4 flex gap-3 rounded-md border-2 border-destructive bg-destructive/10 p-4 text-destructive md:mb-6">
      <TriangleAlert className="mt-0.5 size-6 shrink-0" />
      <div className="min-w-0 space-y-1">
        <p className="text-base font-semibold">{t('Required plugins are not working')}</p>
        <p className="text-sm">{t('Some features are unavailable until these are running:')}</p>
        <ul className="list-disc space-y-0.5 ps-5 text-sm">
          {broken.map((p) => (
            <li key={p.id}>
              <span className="font-medium">{p.id}</span>
              {p.problem && <span>: {p.problem}</span>}
            </li>
          ))}
        </ul>
        <Link to="/settings" className="inline-block pt-1 text-sm font-medium underline">{t('See plugin settings')}</Link>
      </div>
    </div>
  )
}
