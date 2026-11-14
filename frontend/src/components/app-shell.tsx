import { useQueryClient } from '@tanstack/react-query'
import { ChevronDown, KeyRound, LogOut, Menu, Monitor, Moon, Search, Sun, UserRound } from 'lucide-react'
import { useTheme } from 'next-themes'
import { useEffect, useState, type ReactNode } from 'react'
import { NavLink } from 'react-router-dom'
import { toast } from 'sonner'
import { msg, useI18n } from '@/lib/i18n'
import { useLive } from '@/lib/live-state'
import { logout as endSession, usePhotos, useRole, useSession } from '@/services/session'
import { cn } from '@/lib/utils'
import { ChangePassword } from './change-password'
import { CommandPalette } from './command-palette'
import { UploadPanel } from './files/upload-panel'
import { LanguageMenu } from './language-menu'
import { visibleNav } from './nav'
import { PluginAlert } from './plugin-alert'
import { Button } from './ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from './ui/dropdown-menu'
import { Sheet, SheetContent, SheetHeader, SheetTitle } from './ui/sheet'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'
import { REPO } from '@/lib/links'
import { useModuleScripts } from '@/lib/module-loader'

/** The menu: groups of pages, the open one a tinted pill that runs out from the edge. */
function Nav({ onNavigate }: { onNavigate?: () => void }) {
  const { t } = useI18n()
  const groups = visibleNav(useRole(), usePhotos())
  return (
    <nav className="flex flex-col gap-5 py-4" aria-label={t('Main')}>
      {groups.map(({ title, items }) => (
        <div key={title} className="flex flex-col gap-0.5">
          <p className="px-5 pb-1.5 text-xs font-medium text-muted-foreground">{t(title)}</p>
          {items.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              onClick={onNavigate}
              className={({ isActive }) =>
                cn(
                  'flex items-center gap-3 py-2.5 ps-5 pe-4 text-sm font-medium transition-colors',
                  isActive ? 'bg-primary/15 text-foreground [&>svg]:text-primary' : 'text-muted-foreground hover:bg-sidebar-accent hover:text-foreground',
                )
              }
            >
              <Icon className="size-[18px]" />
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
    <div className="flex h-16 items-center gap-2.5 px-5">
      <img src="/favicon.svg" alt="" className="size-8" />
      <span className="text-lg font-semibold tracking-tight">fafnirx</span>
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
  const [palette, setPalette] = useState(false)
  const [passwordOpen, setPasswordOpen] = useState(false)
  const session = useSession()
  useModuleScripts()
  const version = session?.version
  const mac = typeof navigator !== 'undefined' && /mac|iphone|ipad/i.test(navigator.platform)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setPalette((p) => !p)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  async function logout() {
    try {
      await endSession()
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
      <a href="#main" className="skip-link">{t('Skip to content')}</a>
      <aside className="sticky top-0 hidden h-svh w-60 shrink-0 flex-col overflow-y-auto border-e bg-sidebar md:flex">
        <Brand />
        <Nav />
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-16 items-center gap-2 border-b bg-background/90 px-3 backdrop-blur md:px-6">
          <Button variant="ghost" size="icon" className="md:hidden" onClick={() => setOpen(true)} aria-label={t('Menu')}>
            <Menu className="size-4" />
          </Button>
          <button
            type="button"
            onClick={() => setPalette(true)}
            className="flex h-10 min-w-0 flex-1 max-w-xl items-center gap-2.5 rounded-md bg-muted px-4 text-start text-sm text-muted-foreground transition-colors hover:bg-accent focus-visible:outline-2 focus-visible:outline-ring"
            aria-label={t('Go to')}
          >
            <Search className="size-4 shrink-0" />
            <span className="flex-1 truncate">{t('Go to a page, device or folder…')}</span>
            <kbd className="hidden rounded-md border bg-background px-1.5 py-0.5 text-2xs sm:inline">{mac ? '⌘K' : 'Ctrl K'}</kbd>
          </button>
          <div className="hidden flex-1 md:block" />
          {version && (
            <span className="hidden text-xs text-muted-foreground tabular-nums sm:inline" title={t('Master version')}>
              v{version}
            </span>
          )}
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="flex items-center gap-1.5 px-1 text-xs text-muted-foreground sm:px-2">
                <span className={cn('size-2 rounded-full', live ? 'bg-success' : 'animate-pulse bg-warning')} />
                <span className="sr-only sm:not-sr-only">{live ? t('Live') : t('Reconnecting')}</span>
              </span>
            </TooltipTrigger>
            <TooltipContent>{live ? t('Changes appear as they happen') : t('Updates paused; reconnecting')}</TooltipContent>
          </Tooltip>
          <LanguageMenu />
          <ThemeButton />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" asChild>
                <a href={REPO} target="_blank" rel="noreferrer" aria-label={t('fafnirx on GitHub')}><GitHubMark className="size-4" /></a>
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t('fafnirx on GitHub')}</TooltipContent>
          </Tooltip>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="sm" className="gap-1.5 px-2" aria-label={t('Logged in as {name}', { name: session?.user?.name ?? '' })}>
                <UserRound className="size-4" />
                <span className="hidden max-w-32 truncate text-xs sm:inline">{session?.user?.name}</span>
                <ChevronDown className="size-3.5 text-muted-foreground" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="min-w-48">
              <p className="px-2 py-1.5 text-xs text-muted-foreground">{t('Logged in as {name}', { name: session?.user?.name ?? '' })}</p>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={() => setPasswordOpen(true)}>
                <KeyRound />
                {t('Change password')}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void logout()}>
                <LogOut />
                {t('Log out')}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </header>
        <main id="main" tabIndex={-1} className="min-w-0 flex-1 p-3 outline-none md:p-6">
          <PluginAlert />
          {children}
        </main>
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
      <CommandPalette open={palette} onOpenChange={setPalette} />
      <ChangePassword open={passwordOpen} onClose={() => setPasswordOpen(false)} />
      <UploadPanel />
    </div>
  )
}
