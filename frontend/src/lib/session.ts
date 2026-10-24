import { useQuery } from '@tanstack/react-query'
import { api } from './api'
import type { Role, Session } from './types'

/** The device that holds the master files: they belong to nobody and everybody can see them. */
export const MASTER = '_master'

/** Who is logged in. The app only renders behind a login, so there always is a user. */
export function useSession() {
  const { data } = useQuery({ queryKey: ['session'], queryFn: () => api.get<Session>('/api/session'), staleTime: Infinity })
  return data
}

export function useRole(): Role {
  return useSession()?.user?.role ?? 'readonly'
}

/** Whether the role has a workspace of its own: devices, albums, tags and favorites. */
export const hasWorkspace = (role: Role) => role !== 'readonly'

/** The path, as the API names a device in an address: `<user>/<name>` has a slash in it. */
export const deviceAddress = (folder: string) => encodeURIComponent(folder)
