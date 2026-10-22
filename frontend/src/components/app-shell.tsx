import { useQueryClient } from '@tanstack/react-query'
import { Archive, FolderOpen, GalleryVerticalEnd, HardDrive, Heart, Images, LayoutDashboard, LogOut, MapPin, Menu, Monitor, Moon, Server, Settings, Sun, Trash2, UploadCloud } from 'lucide-react'
import { useTheme } from 'next-themes'
import { useState, type ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import { api } from '@/lib/api'
import { useLive } from '@/lib/live-state'
import type { Session } from '@/lib/types'
import { cn } from '@/lib/utils'
import { UploadPanel } from './files/upload-panel'
import { Button } from './ui/button'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from './ui/sheet'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const NAV = [
  {
    title: 'Photos',
    items: [
      { to: '/timeline', label: 'Timeline', icon: Images },
      { to: '/favorites', label: 'Favorites', icon: Heart },
      { to: '/albums', label: 'Albums', icon: GalleryVerticalEnd },
      { to: '/map', label: 'Map', icon: MapPin },
      { to: '/archive', label: 'Archive', icon: Archive },
      { to: '/trash', label: 'Trash', icon: Trash2 },
    ],
  },
  {
    title: 'Backup',
    items: [
      { to: '/', label: 'Dashboard', icon: LayoutDashboard, end: true },
      { to: '/files', label: 'Files', icon: FolderOpen },
      { to: '/devices', label: 'Devices', icon: Monitor },
      { to: '/sources', label: 'Sources', icon: UploadCloud },
      { to: '/mirrors', label: 'Mirrors', icon: HardDrive },
      { to: '/settings', label: 'Settings', icon: Settings },
    ],
  },
]

function Nav({ onNavigate }: { onNavigate?: () => void }) {
  return (
    <nav className="flex flex-col gap-4 p-3" aria-label="Main">
      {NAV.map(({ title, items }) => (
        <div key={title} className="flex flex-col gap-1">
          <p className="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">{title}</p>
          {items.map(({ to, label, icon: Icon, ...rest }) => (
            <NavLink
              key={to}
              to={to}
              end={'end' in rest}
              onClick={onNavigate}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors',
                  isActive ? 'bg-sidebar-accent text-foreground' : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground',
                )
              }
            >
              <Icon className="size-4" />
              {label}
            </NavLink>
          ))}
        </div>
      ))}
    </nav>
  )
}

function Brand() {
  return (
    <div className="flex h-14 items-center gap-2 border-b px-4">
      <div className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">
        <Server className="size-4" />
      </div>
      <span className="font-semibold tracking-tight">fafnirx</span>
    </div>
  )
}

function ThemeButton() {
  const { theme, setTheme } = useTheme()
  const next = theme === 'light' ? 'dark' : theme === 'dark' ? 'system' : 'light'
  const Icon = theme === 'light' ? Sun : theme === 'dark' ? Moon : Monitor
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={() => setTheme(next)} aria-label={`Theme: ${theme ?? 'system'}`}>
          <Icon className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>Theme: {theme ?? 'system'}</TooltipContent>
    </Tooltip>
  )
}

export function AppShell({ children }: { children: ReactNode }) {
  const client = useQueryClient()
  const live = useLive()
  const [open, setOpen] = useState(false)
  const version = client.getQueryData<Session>(['session'])?.version

  async function logout() {
    await api.post('/api/logout').catch(() => undefined)
    client.clear()
    client.setQueryData<Session>(['session'], { admin: false })
  }

  return (
    <div className="flex min-h-svh">
      <aside className="sticky top-0 hidden h-svh w-56 shrink-0 flex-col border-r bg-sidebar md:flex">
        <Brand />
        <Nav />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-14 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur md:px-6">
          <Button variant="ghost" size="icon" className="md:hidden" onClick={() => setOpen(true)} aria-label="Menu">
            <Menu className="size-5" />
          </Button>
          <div className="flex-1" />
          {version && (
            <span className="text-xs text-muted-foreground tabular-nums" title="Master version">
              v{version}
            </span>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="flex items-center gap-2 px-2 text-xs text-muted-foreground">
                <span className={cn('size-2 rounded-full', live ? 'bg-success' : 'bg-warning')} />
                {live ? 'Live' : 'Reconnecting'}
              </span>
            </TooltipTrigger>
            <TooltipContent>{live ? 'Changes appear as they happen' : 'Updates paused; reconnecting'}</TooltipContent>
          </Tooltip>
          <ThemeButton />
          <Button variant="ghost" size="icon" onClick={logout} aria-label="Log out">
            <LogOut className="size-4" />
          </Button>
        </header>
        <main className="min-w-0 flex-1 p-3 md:p-6">{children}</main>
      </div>
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent side="left" className="w-64 p-0">
          <SheetHeader className="sr-only">
            <SheetTitle>Menu</SheetTitle>
          </SheetHeader>
          <Brand />
          <Nav onNavigate={() => setOpen(false)} />
        </SheetContent>
      </Sheet>
      <UploadPanel />
    </div>
  )
}
