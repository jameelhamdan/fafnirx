import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Role, UserInfo } from '@/lib/types'
import { KEYS } from './keys'

export function useUsers(enabled = true) {
  return useQuery({ queryKey: KEYS.users, queryFn: () => api.get<UserInfo[]>('/api/users'), enabled })
}

const address = (name: string) => `/api/users/${encodeURIComponent(name)}`

export const createUser = (body: { name: string; role: Role; password: string }) => api.post('/api/users', body)
export const patchUser = (name: string, body: { role?: Role; disabled?: boolean; password?: string }) => api.patch(address(name), body)
export const removeUser = (name: string) => api.delete(address(name))
