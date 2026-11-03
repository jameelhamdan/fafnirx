import { useQuery } from '@tanstack/react-query'
import { api } from './api'
import { useRole } from './session'
import type { PluginState } from './types'

/** The plugins list (administrators only), shared by the settings page and the warning banner (same query key). */
export function usePlugins() {
  const admin = useRole() === 'admin'
  return useQuery({
    queryKey: ['plugins'],
    queryFn: () => api.get<{ plugins: PluginState[] }>('/api/plugins'),
    enabled: admin,
    refetchInterval: (q) => (q.state.data?.plugins.some((p) => p.state !== 'up') ? 5000 : 15000),
  })
}
