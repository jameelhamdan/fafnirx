import { useInfiniteQuery, useQuery } from '@tanstack/react-query'
import { api, query } from '@/lib/api'
import type { FileInfo, Listing } from '@/lib/types'
import { KEYS } from './keys'

export type ListingParams = { folder: string; path: string; q: string; type: string; sort: string; order: string; showDeleted: boolean }

/** A folder of the files browser, a page at a time (folders first, then files). */
export function useFileListing({ folder, path, q, type, sort, order, showDeleted }: ListingParams) {
  return useInfiniteQuery({
    queryKey: [...KEYS.files, folder, path, q, type, sort, order, showDeleted],
    initialPageParam: 0,
    queryFn: ({ pageParam }) =>
      api.get<Listing>(`/api/files${query({ folder, path, q, type: type === 'all' ? '' : type, sort, order, deleted: showDeleted, offset: pageParam })}`),
    getNextPageParam: (last, pages) => {
      if (last.folder === null) return undefined
      const loaded = pages.reduce((n, p) => n + p.entries.filter((e) => e.kind === 'file').length, 0)
      return loaded < last.total ? loaded : undefined
    },
    placeholderData: (previous) => previous,
  })
}

/** The top of the files browser: the places a file may be put in. */
export function usePlaces() {
  return useQuery({ queryKey: KEYS.places, queryFn: () => api.get<Listing>('/api/files') })
}

export function useFileInfo(folder: string, path: string | null) {
  return useQuery({
    queryKey: [...KEYS.info, folder, path],
    queryFn: () => api.get<FileInfo>(`/api/files/info?folder=${encodeURIComponent(folder)}&path=${encodeURIComponent(path ?? '')}`),
    enabled: path !== null,
  })
}

export const deleteFiles = (items: { folder: string; path: string }[]) => api.post<{ deleted: number }>('/api/files/delete', { items })
export const restoreFile = (file: { folder: string; path: string; version: number }) => api.post('/api/files/restore', file)
export const moveFile = (folder: string, from: string, to: string) => api.post<{ moved: number }>('/api/files/move', { folder, from, to })
export const rescanFiles = () => api.post<{ started: boolean }>('/api/files/rescan')
