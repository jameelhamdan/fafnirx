import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { DeviceDetail, DeviceSummary } from '@/lib/types'
import { KEYS } from './keys'
import { deviceAddress } from './session'

export function useDevices(enabled = true) {
  return useQuery({ queryKey: KEYS.devices, queryFn: () => api.get<DeviceSummary[]>('/api/devices'), enabled })
}

/** One device by its address (`<user>/<name>`). */
export function useDevice(key: string) {
  return useQuery({ queryKey: KEYS.device(key), queryFn: () => api.get<DeviceDetail>(`/api/devices/${deviceAddress(key)}`) })
}

export const createDevice = (name: string, user?: string) => api.post<DeviceDetail>('/api/devices', { name, user })
export const rotateToken = (key: string) => api.post<DeviceDetail>(`/api/devices/${deviceAddress(key)}/token`)
export const releaseDevice = (key: string) => api.post(`/api/devices/${deviceAddress(key)}/release`)
export const revokeDevice = (key: string) => api.post(`/api/devices/${deviceAddress(key)}/revoke`)
