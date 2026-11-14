import { useInfiniteQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Trash } from '@/lib/types'
import { KEYS } from './keys'

/** The deleted files, a page at a time. */
export function useTrash() {
  return useInfiniteQuery({
    queryKey: KEYS.trash,
    initialPageParam: 0,
    queryFn: ({ pageParam }) => api.get<Trash>(`/api/trash?offset=${pageParam}`),
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.entries.length, 0)
      return loaded < last.total ? loaded : undefined
    },
  })
}

export const purgeTrash = (ids: string[]) => api.post<{ files: number; bytes: number }>('/api/trash/purge', { ids })
