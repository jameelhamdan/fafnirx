import { useQuery } from '@tanstack/react-query'
import { ChevronRight, Folder, FolderSync, HardDrive, Laptop, User } from 'lucide-react'
import { Fragment } from 'react'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import type { ScopeChoice } from '@/lib/types'

const ICON: Record<string, typeof Folder> = { master: HardDrive, user: User, device: Laptop, shared: FolderSync, local: Folder, source: Folder, folder: Folder }

/**
 * Picks what a mirror holds: everything, or one folder found by going down the tree the way the files are stored
 * (a user, one of their devices or shared folders, then folders inside it; or the master files). The folder shown
 * in the path is the choice.
 */
export function ScopePicker({ value, onChange }: { value: string; onChange: (scope: string) => void }) {
  const t = useT()
  const parts = value === '' ? [] : value.split('/')
  const { data, error } = useQuery({
    queryKey: ['mirror-scope', value],
    queryFn: () => api.get<ScopeChoice[]>(`/api/mirrors/scope?path=${encodeURIComponent(value)}`),
  })
  return (
    <div className="space-y-2 rounded-md border p-3">
      <nav aria-label={t('Folder to mirror')} className="flex flex-wrap items-center gap-1 text-sm">
        <button type="button" className={parts.length === 0 ? 'font-medium' : 'hover:underline'} onClick={() => onChange('')}>{t('Everything')}</button>
        {parts.map((part, i) => (
          <Fragment key={i}>
            <ChevronRight className="size-3.5 text-muted-foreground" />
            <button type="button" className={i === parts.length - 1 ? 'font-medium' : 'hover:underline'} onClick={() => onChange(parts.slice(0, i + 1).join('/'))}>{part}</button>
          </Fragment>
        ))}
      </nav>
      {error ? (
        <p role="alert" className="text-sm text-destructive">{error.message}</p>
      ) : !data ? (
        <Skeleton className="h-16" />
      ) : data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{parts.length === 0 ? t('There is nothing to mirror yet.') : t('No folders inside. Everything in it is mirrored.')}</p>
      ) : (
        <ul className="max-h-40 space-y-0.5 overflow-auto">
          {data.map((c) => {
            const Icon = ICON[c.kind] ?? Folder
            return (
              <li key={c.name}>
                <button type="button" className="flex w-full items-center gap-2 rounded px-2 py-1 text-start text-sm hover:bg-muted" onClick={() => onChange(value === '' ? c.name : `${value}/${c.name}`)}>
                  <Icon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="truncate">{c.name}</span>
                </button>
              </li>
            )
          })}
        </ul>
      )}
      <p className="text-xs text-muted-foreground">
        {value === '' ? t('Mirrors everything: the master files and every user.') : t('Mirrors only {folder} and what is in it. Click a folder to go into it, a name above to go back.', { folder: value })}
      </p>
    </div>
  )
}
