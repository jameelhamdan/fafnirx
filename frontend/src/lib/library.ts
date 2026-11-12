import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useApiMutation } from './mutation'
import { toast } from 'sonner'
import { api, query } from './api'
import { count } from './format'
import type { AlbumInfo, Asset, Scope, TagInfo } from './types'
import { useT } from '@/lib/i18n'

/** Everything that shows library data: refreshed after any change. */
const LIBRARY_KEYS = ['timeline', 'asset', 'folder', 'albums', 'album', 'tags', 'trash', 'files', 'info'].map((key) => [key])

export function useRefreshLibrary() {
  const client = useQueryClient()
  return () => Promise.all(LIBRARY_KEYS.map((queryKey) => client.invalidateQueries({ queryKey })))
}

export interface Edit {
  favorite?: boolean
  archived?: boolean
  rating?: number | null
  description?: string
  /** Unix seconds; null goes back to what the camera said. */
  taken_at?: number | null
  location?: { lat: number; lon: number } | null
  tags?: string[]
}

/** The changes people make to photos and videos, each with a toast on failure and a refresh on success. */
export function useLibraryActions() {
  const t = useT()
  const refresh = useRefreshLibrary()

  const edit = useApiMutation(({ id, ...body }: Edit & { id: string }) => api.patch(`/api/library/${id}`, body), {
    invalidate: LIBRARY_KEYS,
  })
  const bulk = useApiMutation((body: { ids: string[]; favorite?: boolean; archived?: boolean; add_tags?: string[] }) =>
      api.post<{ changed: number }>('/api/library/bulk', body), {
    invalidate: LIBRARY_KEYS,
  })
  const remove = useApiMutation((items: { folder: string; path: string }[]) => api.post<{ deleted: number }>('/api/files/delete', { items }), {
    onSuccess: (r) => {
      toast.success(t('{count} deleted. They stay in the trash until the retention period ends.', { count: count(r.deleted) }))
      void refresh()
    },
  })
  return { edit, bulk, remove }
}

export function useAlbums(enabled = true) {
  return useQuery({ queryKey: ['albums'], queryFn: () => api.get<AlbumInfo[]>('/api/albums'), enabled })
}

export function useTags(enabled = true) {
  return useQuery({ queryKey: ['tags'], queryFn: () => api.get<TagInfo[]>('/api/tags'), enabled })
}

/** What a view of the timeline is filtered by. */
export type TimelineFilters = {
  scope: Scope
  folder: string
  kind: string
  tag: string
}

/** One month of a timeline view. The page and the viewer share these queries, so the viewer finds the months already loaded. */
export function monthQuery(filters: TimelineFilters, month: string) {
  return {
    queryKey: ['timeline', 'month', filters, month],
    queryFn: () => api.get<{ assets: Asset[] }>(`/api/timeline${query({ ...filters, month })}`),
    staleTime: 30_000,
  }
}
