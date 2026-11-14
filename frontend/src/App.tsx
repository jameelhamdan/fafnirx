import { useQueryClient } from '@tanstack/react-query'
import { lazy, Suspense, useEffect, type ReactNode } from 'react'
import { Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { AppShell } from './components/app-shell.tsx'
import { UNAUTHORIZED } from './lib/api.ts'
import { LiveUpdates } from './lib/live.tsx'
import { backgroundOf } from './lib/viewer-source'
import { ViewerSourceProvider } from './components/viewer-source-provider'
import { hasWorkspace, useSessionQuery } from './services/session.ts'
import { KEYS } from './services/keys.ts'
import type { Role, Session } from './lib/types.ts'
import { Skeleton } from './components/ui/skeleton.tsx'
import { Button } from './components/ui/button.tsx'
import { ErrorNote } from './components/common.tsx'
import { useT } from './lib/i18n.ts'
import { Loader2 } from 'lucide-react'
import Login from './pages/login.tsx'

// Each page is loaded when it is first opened, so the first screen stays small.
const Dashboard = lazy(() => import('./pages/dashboard.tsx'))
const DeviceDetailPage = lazy(() => import('./pages/device.tsx'))
const Devices = lazy(() => import('./pages/devices.tsx'))
const Files = lazy(() => import('./pages/files.tsx'))
const Mirrors = lazy(() => import('./pages/mirrors.tsx'))
const Settings = lazy(() => import('./pages/settings.tsx'))
const Queue = lazy(() => import('./pages/queue.tsx'))
const Sources = lazy(() => import('./pages/sources.tsx'))
const Timeline = lazy(() => import('./pages/timeline.tsx'))
const Albums = lazy(() => import('./pages/albums.tsx'))
const Album = lazy(() => import('./pages/album.tsx'))
const Watch = lazy(() => import('./pages/watch.tsx'))
const Trash = lazy(() => import('./pages/trash.tsx'))
const Users = lazy(() => import('./pages/users.tsx'))

/** A page for some of the roles; the others are sent to the first page. */
function Only({ allow, role, children }: { allow: 'admin' | 'workspace'; role: Role; children: ReactNode }) {
  const ok = allow === 'admin' ? role === 'admin' : hasWorkspace(role)
  return ok ? children : <Navigate to="/" replace />
}

/** A photo page: closed (sent to the first page) when the administrator turned the photo pages off. */
function Photos({ on, children }: { on: boolean; children: ReactNode }) {
  return on ? children : <Navigate to="/" replace />
}

function Centered({ children }: { children: ReactNode }) {
  return <div className="grid min-h-svh place-items-center p-4">{children}</div>
}

/** The login page, or the app behind it. */
export default function App() {
  const t = useT()
  const client = useQueryClient()
  const session = useSessionQuery()

  // Any call that finds the login ended sends the user back to the login page.
  useEffect(() => {
    const ended = () => client.setQueryData<Session>(KEYS.session, { user: null })
    window.addEventListener(UNAUTHORIZED, ended)
    return () => window.removeEventListener(UNAUTHORIZED, ended)
  }, [client])

  const location = useLocation()
  if (session.isPending) return <Centered><Loader2 className="size-6 animate-spin text-muted-foreground" aria-label={t('Loading')} /></Centered>
  // The master did not answer: that is not a reason to ask for a password.
  if (session.isError && !session.data) {
    return (
      <Centered>
        <div className="max-w-sm space-y-3 text-center">
          <ErrorNote error={new Error(t('The master cannot be reached: {why}', { why: session.error.message }))} />
          <Button variant="outline" onClick={() => void session.refetch()} disabled={session.isFetching}>{t('Try again')}</Button>
        </div>
      </Centered>
    )
  }
  const user = session.data?.user
  if (!user) return <Login />
  const role = user.role
  const photos = session.data?.photos !== false
  // A viewer opened from a page is shown over it (see `viewer-source`): the page is the one it was opened from.
  const background = backgroundOf(location)

  return (
    <ViewerSourceProvider>
    <LiveUpdates>
      <AppShell>
        <Suspense fallback={<Skeleton className="h-64" />}>
          <Routes location={background ?? location}>
            <Route path="/" element={<Dashboard />} />
            {/* A key per view, so that a selection or an open photo does not carry over from one to another. */}
            <Route path="/timeline" element={<Photos on={photos}><Timeline key="timeline" scope="timeline" /></Photos>} />
            <Route path="/favorites" element={<Only allow="workspace" role={role}><Photos on={photos}><Timeline key="favorites" scope="favorites" /></Photos></Only>} />
            <Route path="/archive" element={<Only allow="workspace" role={role}><Photos on={photos}><Timeline key="archive" scope="archive" /></Photos></Only>} />
            <Route path="/albums" element={<Only allow="workspace" role={role}><Photos on={photos}><Albums /></Photos></Only>} />
            <Route path="/albums/:id" element={<Only allow="workspace" role={role}><Photos on={photos}><Album /></Photos></Only>} />
            <Route path="/watch/:id" element={<Watch />} />
            <Route path="/trash" element={<Only allow="workspace" role={role}><Trash /></Only>} />
            <Route path="/files" element={<Files />} />
            <Route path="/devices" element={<Only allow="workspace" role={role}><Devices /></Only>} />
            <Route path="/devices/:name" element={<Only allow="workspace" role={role}><DeviceDetailPage /></Only>} />
            <Route path="/queue" element={<Queue />} />
            <Route path="/sources" element={<Only allow="workspace" role={role}><Sources /></Only>} />
            <Route path="/mirrors" element={<Only allow="admin" role={role}><Mirrors /></Only>} />
            <Route path="/settings" element={<Only allow="admin" role={role}><Settings /></Only>} />
            <Route path="/users" element={<Only allow="admin" role={role}><Users /></Only>} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
          {background && (
            <Routes>
              <Route path="/watch/:id" element={<Watch />} />
            </Routes>
          )}
        </Suspense>
      </AppShell>
    </LiveUpdates>
    </ViewerSourceProvider>
  )
}
