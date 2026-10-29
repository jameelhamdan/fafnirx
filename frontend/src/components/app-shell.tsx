import { useQueryClient } from '@tanstack/react-query'
import { Archive, Cog, FolderOpen, FolderSync, GalleryVerticalEnd, HardDrive, Heart, Images, LayoutDashboard, LogOut, MapPin, Menu, Monitor, Moon, Settings, Sun, Trash2, UploadCloud, Users } from 'lucide-react'
import { useTheme } from 'next-themes'
import { useState, type ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import { toast } from 'sonner'
import { api } from '@/lib/api'
import { msg, useI18n } from '@/lib/i18n'
import { useLive } from '@/lib/live-state'
import { hasWorkspace, useRole, useSession } from '@/lib/session'
import type { Role } from '@/lib/types'
import { cn } from '@/lib/utils'
import { UploadPanel } from './files/upload-panel'
import { LanguageMenu } from './language-menu'
import { Button } from './ui/button'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from './ui/sheet'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'
import { REPO } from '@/lib/links'
import { useModuleScripts } from '@/lib/module-loader'

/** Who sees an entry: everybody, the roles with a workspace of their own (not read-only), or administrators. */
type Audience = 'all' | 'workspace' | 'admin'

const NAV: { title: string; items: { to: string; label: string; icon: typeof Images; end?: boolean; for: Audience }[] }[] = [
  {
    title: msg('Photos'),
    items: [
      { to: '/timeline', label: msg('Timeline'), icon: Images, for: 'all' },
      { to: '/favorites', label: msg('Favorites'), icon: Heart, for: 'workspace' },
      { to: '/albums', label: msg('Albums'), icon: GalleryVerticalEnd, for: 'workspace' },
      { to: '/map', label: msg('Map'), icon: MapPin, for: 'all' },
      { to: '/archive', label: msg('Archive'), icon: Archive, for: 'workspace' },
      { to: '/trash', label: msg('Trash'), icon: Trash2, for: 'workspace' },
    ],
  },
  {
    title: msg('Storage'),
    items: [
      { to: '/', label: msg('Dashboard'), icon: LayoutDashboard, end: true, for: 'all' },
      { to: '/files', label: msg('Files'), icon: FolderOpen, for: 'all' },
      { to: '/devices', label: msg('Devices'), icon: Monitor, for: 'workspace' },
      { to: '/shared', label: msg('Shared folders'), icon: FolderSync, for: 'workspace' },
      { to: '/sources', label: msg('Sources'), icon: UploadCloud, for: 'workspace' },
      { to: '/queue', label: msg('Processing'), icon: Cog, for: 'all' },
      { to: '/mirrors', label: msg('Mirrors'), icon: HardDrive, for: 'admin' },
      { to: '/users', label: msg('Users'), icon: Users, for: 'admin' },
      { to: '/settings', label: msg('Settings'), icon: Settings, for: 'all' },
    ],
  },
]

const allowed = (audience: Audience, role: Role) => audience === 'all' || (audience === 'workspace' ? hasWorkspace(role) : role === 'admin')

function Nav({ onNavigate }: { onNavigate?: () => void }) {
  const { t } = useI18n()
  const role = useRole()
  return (
    <nav className="flex flex-col gap-4 p-3" aria-label={t('Main')}>
      {NAV.map(({ title, items: all }) => ({ title, items: all.filter((i) => allowed(i.for, role)) })).filter((g) => g.items.length > 0).map(({ title, items }) => (
        <div key={title} className="flex flex-col gap-1">
          <p className="px-3 pb-1 text-xs font-medium tracking-wide text-muted-foreground uppercase">{t(title)}</p>
          {items.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              onClick={onNavigate}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors',
                  isActive ? 'bg-sidebar-accent text-foreground' : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-foreground',
                )
              }
            >
              <Icon className="size-4" />
              {t(label)}
            </NavLink>
          ))}
        </div>
      ))}
    </nav>
  )
}

function GitHubMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true" className={className}>
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82a7.6 7.6 0 0 1 2-.27c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
    </svg>
  )
}

function Brand() {
  return (
    <div className="flex h-14 items-center gap-2 border-b px-4">
      <img src="/favicon.svg" alt="" className="size-7" />
      <span className="font-semibold tracking-tight">fafnirx</span>
    </div>
  )
}

const THEME: Record<string, string> = { light: msg('light'), dark: msg('dark'), system: msg('system') }

function ThemeButton() {
  const { t } = useI18n()
  const { theme, setTheme } = useTheme()
  const next = theme === 'light' ? 'dark' : theme === 'dark' ? 'system' : 'light'
  const Icon = theme === 'light' ? Sun : theme === 'dark' ? Moon : Monitor
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" onClick={() => setTheme(next)} aria-label={t('Theme: {theme}', { theme: t(THEME[theme ?? 'system']) })}>
          <Icon className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t('Theme: {theme}', { theme: t(THEME[theme ?? 'system']) })}</TooltipContent>
    </Tooltip>
  )
}

export function AppShell({ children }: { children: ReactNode }) {
  const { t } = useI18n()
  const client = useQueryClient()
  const live = useLive()
  const [open, setOpen] = useState(false)
  const session = useSession()
  useModuleScripts()
  const version = session?.version

  async function logout() {
    try {
      await api.post('/api/logout')
    } catch (e) {
      // Still logged in: say so instead of showing a login page the server would not honour.
      toast.error(e instanceof Error ? e.message : t('Could not log out'))
      return
    }
    client.clear()
    // A full load of the start page, which is the login page without a session: nothing of this user stays in memory.
    window.location.replace('/')
  }

  return (
    <div className="flex min-h-svh">
      <aside className="sticky top-0 hidden h-svh w-56 shrink-0 flex-col border-r bg-sidebar md:flex">
        <Brand />
        <Nav />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-14 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur md:px-6">
          <Button variant="ghost" size="icon" className="md:hidden" onClick={() => setOpen(true)} aria-label={t('Menu')}>
            <Menu className="size-5" />
          </Button>
          <div className="flex-1" />
          {version && (
            <span className="text-xs text-muted-foreground tabular-nums" title={t('Master version')}>
              v{version}
            </span>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="flex items-center gap-2 px-2 text-xs text-muted-foreground">
                <span className={cn('size-2 rounded-full', live ? 'bg-success' : 'bg-warning')} />
                {live ? t('Live') : t('Reconnecting')}
              </span>
            </TooltipTrigger>
            <TooltipContent>{live ? t('Changes appear as they happen') : t('Updates paused; reconnecting')}</TooltipContent>
          </Tooltip>
          <LanguageMenu />
          <ThemeButton />
          <span className="hidden px-1 text-xs text-muted-foreground sm:inline" title={t('Logged in as {name}', { name: session?.user?.name ?? '' })}>
            {session?.user?.name}
          </span>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" asChild>
                <a href={REPO} target="_blank" rel="noreferrer" aria-label={t('fafnirx on GitHub')}><GitHubMark className="size-4" /></a>
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t('fafnirx on GitHub')}</TooltipContent>
          </Tooltip>
          <Button variant="ghost" size="icon" onClick={logout} aria-label={t('Log out')}>
            <LogOut className="size-4" />
          </Button>
        </header>
        <main className="min-w-0 flex-1 p-3 md:p-6">{children}</main>
      </div>
      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent side="left" className="w-64 p-0">
          <SheetHeader className="sr-only">
            <SheetTitle>{t('Menu')}</SheetTitle>
          </SheetHeader>
          <Brand />
          <Nav onNavigate={() => setOpen(false)} />
        </SheetContent>
      </Sheet>
      <UploadPanel />
    </div>
  )
}
