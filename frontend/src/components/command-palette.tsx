import { CornerDownLeft, FolderOpen, Monitor, Search } from 'lucide-react'
import { useEffect, useMemo, useRef, useState, type ComponentType } from 'react'
import { useNavigate } from 'react-router-dom'
import { useDevices } from '@/services/devices'
import { useFolders } from '@/services/folders'
import { useT } from '@/lib/i18n'
import { folderLink } from '@/lib/links'
import { deviceAddress, hasWorkspace, usePhotos, useRole } from '@/services/session'
import { cn } from '@/lib/utils'
import { matches } from '@/lib/table-view'
import { visibleNav } from './nav'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from './ui/dialog'

type Hit = { key: string; group: string; label: string; hint?: string; icon: ComponentType<{ className?: string }>; to: string }

/** Jump to a page, a device or a folder by typing a few letters of its name. Opened from the top bar or with Ctrl/⌘ K. */
export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const t = useT()
  const role = useRole()
  const photos = usePhotos()
  const navigate = useNavigate()
  const [query, setQuery] = useState('')
  const [active, setActive] = useState(0)
  const list = useRef<HTMLUListElement>(null)
  const workspace = hasWorkspace(role)
  const devices = useDevices(open && workspace)
  const folders = useFolders(open && workspace)

  const hits = useMemo(() => {
    const all: Hit[] = [
      ...visibleNav(role, photos).flatMap((g) => g.items.map((i) => ({ key: i.to, group: t('Pages'), label: t(i.label), icon: i.icon, to: i.to }))),
      ...(devices.data ?? []).map((d) => ({ key: `d:${d.key}`, group: t('Devices'), label: d.key, hint: d.online ? t('Online') : t('Offline'), icon: Monitor, to: `/devices/${deviceAddress(d.key)}` })),
      ...(folders.data ?? []).map((f) => ({ key: `f:${f.key}`, group: t('Folders'), label: f.key, icon: FolderOpen, to: folderLink(f.key) })),
    ]
    return all.filter((h) => matches(query, h.label, h.group)).slice(0, 40)
  }, [query, role, photos, devices.data, folders.data, t])

  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [active])

  function close(next: boolean) {
    if (!next) {
      setQuery('')
      setActive(0)
    }
    onOpenChange(next)
  }

  function go(hit: Hit | undefined) {
    if (!hit) return
    close(false)
    navigate(hit.to)
  }

  return (
    <Dialog open={open} onOpenChange={close}>
      <DialogContent showCloseButton={false} className="top-[18%] translate-y-0 gap-0 overflow-hidden p-0 sm:max-w-xl">
        <DialogTitle className="sr-only">{t('Go to')}</DialogTitle>
        <DialogDescription className="sr-only">{t('Type to find a page, a device or a folder.')}</DialogDescription>
        <div className="flex items-center gap-2 border-b px-3">
          <Search className="size-4 shrink-0 text-muted-foreground" />
          <input
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-label={t('Go to')}
            value={query}
            placeholder={t('Go to a page, device or folder…')}
            onChange={(e) => { setQuery(e.target.value); setActive(0) }}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') { e.preventDefault(); setActive((a) => Math.min(a + 1, hits.length - 1)) }
              else if (e.key === 'ArrowUp') { e.preventDefault(); setActive((a) => Math.max(a - 1, 0)) }
              else if (e.key === 'Enter') { e.preventDefault(); go(hits[active]) }
            }}
            className="h-11 w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </div>
        <ul id="palette-list" ref={list} role="listbox" className="max-h-80 overflow-y-auto p-1">
          {hits.length === 0 && <li className="px-3 py-6 text-center text-sm text-muted-foreground">{t('Nothing matches.')}</li>}
          {hits.map((h, i) => (
            <li key={h.key} role="presentation">
              {(i === 0 || hits[i - 1].group !== h.group) && <p className="px-2 pt-2 pb-1 text-xs font-medium text-muted-foreground">{h.group}</p>}
              <button
                type="button"
                role="option"
                aria-selected={i === active}
                onMouseMove={() => setActive(i)}
                onClick={() => go(h)}
                className={cn('flex w-full items-center gap-2 rounded-lg px-2 py-2 text-start text-sm', i === active && 'bg-accent')}
              >
                <h.icon className="size-4 shrink-0 text-muted-foreground" />
                <span className={cn('min-w-0 flex-1 truncate', h.group !== t('Pages') && 'num text-compact')}>{h.label}</span>
                {h.hint && <span className="text-xs text-muted-foreground">{h.hint}</span>}
                {i === active && <CornerDownLeft className="size-3.5 text-muted-foreground" />}
              </button>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  )
}
