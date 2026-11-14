import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { api, query } from '@/lib/api'
import { count } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useApiMutation } from '@/lib/mutation'
import type { AlbumInfo, Asset, AssetDetail, Buckets, Scope, TagInfo } from '@/lib/types'
import { KEYS, LIBRARY_KEYS } from './keys'

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

export const editAsset = ({ id, ...body }: Edit & { id: string }) => api.patch(`/api/library/${id}`, body)
export const bulkEdit = (body: { ids: string[]; favorite?: boolean; archived?: boolean; add_tags?: string[] }) =>
  api.post<{ changed: number }>('/api/library/bulk', body)

/** The changes people make to photos and videos, each with a toast on failure and a refresh on success. */
export function useLibraryActions() {
  const t = useT()
  const refresh = useRefreshLibrary()
  const edit = useApiMutation(editAsset, { invalidate: LIBRARY_KEYS })
  const bulk = useApiMutation(bulkEdit, { invalidate: LIBRARY_KEYS })
  const remove = useApiMutation(
    (items: { folder: string; path: string }[]) => api.post<{ deleted: number }>('/api/files/delete', { items }),
    {
      onSuccess: (r) => {
        toast.success(t('{count} deleted. They stay in the trash until the retention period ends.', { count: count(r.deleted) }))
        void refresh()
      },
    },
  )
  return { edit, bulk, remove }
}

/** One photo or video with everything known about it. The viewer, the details sheet and the info panel share this query. */
export function useAsset(id: string) {
  return useQuery({ queryKey: KEYS.asset(id), queryFn: () => api.get<AssetDetail>(`/api/library/${id}`) })
}

/** The other files of the folder that holds a library file: what the viewer steps through when opened from a folder. */
export function useFolderAssets(id: string, enabled: boolean) {
  return useQuery({
    queryKey: ['folder', id],
    queryFn: () => api.get<{ assets: Asset[] }>(`/api/library/${id}/folder`),
    enabled,
  })
}

export function useAlbums(enabled = true) {
  return useQuery({ queryKey: KEYS.albums, queryFn: () => api.get<AlbumInfo[]>('/api/albums'), enabled })
}

export function useTags(enabled = true) {
  return useQuery({ queryKey: KEYS.tags, queryFn: () => api.get<TagInfo[]>('/api/tags'), enabled })
}

export function useAlbum(id: string) {
  return useQuery({
    queryKey: [...KEYS.album, id],
    queryFn: () => api.get<{ album: AlbumInfo; assets: Asset[]; more: boolean }>(`/api/albums/${id}`),
    enabled: id !== '',
  })
}

/** A folder of a device or of the master: the device or folder name, and a path inside it. */
export interface FolderRef {
  folder: string
  path: string
}

export const createAlbum = (body: { name: string; folder: FolderRef }) => api.post('/api/albums', body)
export const patchAlbum = (id: string, body: { name?: string; description?: string; cover?: string }) => api.patch(`/api/albums/${id}`, body)
export const removeAlbum = (id: string) => api.delete(`/api/albums/${id}`)

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
    queryKey: [...KEYS.timeline, 'month', filters, month],
    queryFn: () => api.get<{ assets: Asset[] }>(`/api/timeline${query({ ...filters, month })}`),
    staleTime: 30_000,
  }
}

export function useTimelineBuckets(filters: TimelineFilters) {
  return useQuery({
    queryKey: [...KEYS.timeline, 'buckets', filters],
    queryFn: () => api.get<Buckets>(`/api/timeline/buckets${query(filters)}`),
    placeholderData: keepPreviousData,
  })
}
