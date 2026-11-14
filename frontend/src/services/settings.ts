import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { PluginState, Settings } from '@/lib/types'
import { KEYS } from './keys'
import { useRole } from './session'

export function useSettings() {
  return useQuery({ queryKey: KEYS.settings, queryFn: () => api.get<Settings>('/api/settings') })
}

/** The server takes all the settings at once. */
export const saveSettings = (settings: Settings) => api.put<Settings>('/api/settings', settings)

/** The plugins list (administrators only), shared by the settings page and the warning banner (same query key). */
export function usePlugins() {
  const admin = useRole() === 'admin'
  return useQuery({
    queryKey: KEYS.plugins,
    queryFn: () => api.get<{ plugins: PluginState[] }>('/api/plugins'),
    enabled: admin,
    refetchInterval: (q) => (q.state.data?.plugins.some((p) => p.state !== 'up') ? 5000 : 15000),
  })
}
