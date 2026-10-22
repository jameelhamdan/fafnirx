import { useQuery, useQueryClient } from '@tanstack/react-query'
import { lazy, Suspense, useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { AppShell } from './components/app-shell.tsx'
import { api, UNAUTHORIZED } from './lib/api.ts'
import { LiveUpdates } from './lib/live.tsx'
import type { Session } from './lib/types.ts'
import { Skeleton } from './components/ui/skeleton.tsx'
import Login from './pages/login.tsx'

// Each page is loaded when it is first opened, so the first screen stays small.
const Dashboard = lazy(() => import('./pages/dashboard.tsx'))
const DeviceDetailPage = lazy(() => import('./pages/device.tsx'))
const Devices = lazy(() => import('./pages/devices.tsx'))
const Files = lazy(() => import('./pages/files.tsx'))
const Mirrors = lazy(() => import('./pages/mirrors.tsx'))
const Settings = lazy(() => import('./pages/settings.tsx'))
const Sources = lazy(() => import('./pages/sources.tsx'))
const Timeline = lazy(() => import('./pages/timeline.tsx'))
const Albums = lazy(() => import('./pages/albums.tsx'))
const Album = lazy(() => import('./pages/album.tsx'))
const MapPage = lazy(() => import('./pages/map.tsx'))
const Trash = lazy(() => import('./pages/trash.tsx'))

/** The login page, or the app behind it. */
export default function App() {
  const client = useQueryClient()
  const session = useQuery({ queryKey: ['session'], queryFn: () => api.get<Session>('/api/session'), staleTime: Infinity })

  // Any call that finds the login ended sends the user back to the login page.
  useEffect(() => {
    const ended = () => client.setQueryData(['session'], { admin: false })
    window.addEventListener(UNAUTHORIZED, ended)
    return () => window.removeEventListener(UNAUTHORIZED, ended)
  }, [client])

  if (session.isPending) return null
  if (!session.data?.admin) return <Login />

  return (
    <LiveUpdates>
      <AppShell>
        <Suspense fallback={<Skeleton className="h-64" />}>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            {/* A key per view, so that a selection or an open photo does not carry over from one to another. */}
            <Route path="/timeline" element={<Timeline key="timeline" scope="timeline" />} />
            <Route path="/favorites" element={<Timeline key="favorites" scope="favorites" />} />
            <Route path="/archive" element={<Timeline key="archive" scope="archive" />} />
            <Route path="/albums" element={<Albums />} />
            <Route path="/albums/:id" element={<Album />} />
            <Route path="/map" element={<MapPage />} />
            <Route path="/trash" element={<Trash />} />
            <Route path="/files" element={<Files />} />
            <Route path="/devices" element={<Devices />} />
            <Route path="/devices/:name" element={<DeviceDetailPage />} />
            <Route path="/sources" element={<Sources />} />
            <Route path="/mirrors" element={<Mirrors />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Suspense>
      </AppShell>
    </LiveUpdates>
  )
}
