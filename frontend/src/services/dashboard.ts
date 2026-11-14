import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Dashboard } from '@/lib/types'
import { KEYS } from './keys'

export function useDashboard() {
  return useQuery({ queryKey: KEYS.dashboard, queryFn: () => api.get<Dashboard>('/api/dashboard') })
}

export const dismissAlert = (id: string) => api.post(`/api/alerts/${id}/dismiss`)
