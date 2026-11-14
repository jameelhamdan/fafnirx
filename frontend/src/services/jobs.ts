import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { JobState, QueueState } from '@/lib/types'
import { KEYS } from './keys'

/** Jobs that run on a schedule: what is read from elsewhere (sources) and what is copied elsewhere (mirrors). */
export type JobKind = 'sources' | 'mirrors'

export function useJobs(kind: JobKind) {
  return useQuery({ queryKey: KEYS[kind], queryFn: () => api.get<JobState[]>(`/api/${kind}`) })
}

export const runJob = (kind: JobKind, key: string, what: 'run' | 'toggle') => api.post(`/api/${kind}/${key}/${what}`)
export const removeJob = (kind: JobKind, key: string) => api.delete(`/api/${kind}/${key}`)

export const createSource = (body: unknown) => api.post('/api/sources', body)
export const patchSource = (key: string, body: unknown) => api.patch(`/api/sources/${key}`, body)

export function useMirrorScopes() {
  return useQuery({ queryKey: KEYS.mirrorScopes, queryFn: () => api.get<string[]>('/api/mirrors/scopes') })
}
export const createMirror = (body: unknown) => api.post('/api/mirrors', body)
export const resyncMirror = (name: string) => api.post(`/api/mirrors/${name}/resync`)
export const restoreKit = (name: string) => api.get<{ config: string }>(`/api/mirrors/${name}/restore-kit`).then((r) => ({ name, config: r.config }))

/** The processing queue: pictures, details and what the modules work on. */
export function useQueue() {
  return useQuery({ queryKey: KEYS.queue, queryFn: () => api.get<QueueState>('/api/queue'), refetchInterval: 3000 })
}

type Changed = { changed: number } | null
export const retryJobs = (id?: string) => api.post<Changed>('/api/queue/retry', { id })
export const cancelJobs = (status: 'failed' | 'queued', id?: string) => api.post<Changed>('/api/queue/cancel', { status, id })
export const pauseQueue = (paused: boolean) => api.post<Changed>('/api/queue/pause', { paused })
export const rebuildPictures = (everything: boolean) => api.post<Changed>('/api/queue/rebuild', { everything })
