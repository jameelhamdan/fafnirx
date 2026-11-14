import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Folder } from '@/lib/types'
import { KEYS } from './keys'

export type Person = { name: string; can_write: boolean }
export type Member = { user: string; can_write: boolean }

export function useFolders(enabled = true) {
  return useQuery({ queryKey: KEYS.folders, queryFn: () => api.get<Folder[]>('/api/folders'), enabled })
}

/** The users a folder may be shared with. */
export function useFolderPeople() {
  return useQuery({ queryKey: KEYS.folderPeople, queryFn: () => api.get<Person[]>('/api/folders/people'), staleTime: 60_000 })
}

const address = (name: string) => `/api/folders/${encodeURIComponent(name)}`

export const createFolder = (name: string): Promise<{ folder: string }> => api.post('/api/folders', { name })
export const renameFolder = (current: string, name: string) => api.post<{ folder: string }>(`${address(current)}/rename`, { name })
export const removeFolder = (name: string) => api.delete(address(name))
export const leaveFolder = (key: string) => api.post('/api/folders/leave', { folder: key })
/** Adds a user to a folder, or changes what they may do: the same request. */
export const shareFolder = (folder: string, member: Member) => api.post(`${address(folder)}/members`, member)
export const unshareFolder = (folder: string, user: string) => api.delete(`${address(folder)}/members/${encodeURIComponent(user)}`)

export const createMasterFolder = (path: string) => api.post('/api/master-folders', { path })
export const renameMasterFolder = (id: string, path: string) => api.post(`/api/master-folders/${id}/rename`, { path }).then(() => ({ folder: path }))
export const removeMasterFolder = (id: string) => api.delete(`/api/master-folders/${id}`)
