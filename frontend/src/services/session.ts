import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Role, Session } from '@/lib/types'
import { KEYS } from './keys'

/** The session query itself, for the one place that must tell "still loading" from "nobody logged in". */
export function useSessionQuery() {
  return useQuery({ queryKey: KEYS.session, queryFn: () => api.get<Session>('/api/session'), staleTime: Infinity })
}

/** Who is logged in. The app only renders behind a login, so there always is a user. */
export function useSession() {
  return useSessionQuery().data
}

export function useRole(): Role {
  return useSession()?.user?.role ?? 'readonly'
}

/** Whether the administrator offers the photo pages (timeline, favorites, albums, archive). */
export function usePhotos() {
  return useSession()?.photos !== false
}

/** Whether the role has a workspace of its own: devices, albums, tags and favorites. */
export const hasWorkspace = (role: Role) => role !== 'readonly'

/** The path, as the API names a device in an address: `<user>/<name>` has a slash in it. */
export const deviceAddress = (folder: string) => encodeURIComponent(folder)

export const fetchSession = () => api.get<Session>('/api/session')
export const login = (username: string, password: string) => api.post('/api/login', { username, password })
export const logout = () => api.post('/api/logout')
export const changePassword = (current: string, next: string) => api.post('/api/me/password', { current, new: next })
