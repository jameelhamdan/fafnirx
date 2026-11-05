import { useQuery } from '@tanstack/react-query'
import { SELECT } from '@/components/common'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import type { Listing } from '@/lib/types'

/**
 * Picks the folder a source writes into: one of your folders, a shared folder you may change, or a master folder. `own` offers a
 * new folder of the source's own (when it is added); its value is then the empty string.
 */
export function FolderPlace({ value, onChange, own }: { value: string; onChange: (next: string) => void; own?: boolean }) {
  const t = useT()
  const places = useQuery({ queryKey: ['files', 'places'], queryFn: () => api.get<Listing>('/api/files') })
  const folders = (places.data?.entries ?? []).filter((e) => e.kind === 'device' && e.writable && ['local', 'master'].includes(e.device_kind ?? ''))
  return (
    <div className="space-y-1.5">
      <Label htmlFor="place-device">{t('Where the files go')}</Label>
      <select id="place-device" className={`${SELECT} w-full`} value={value} onChange={(e) => onChange(e.target.value)}>
        {own && <option value="">{t('A new folder of its own')}</option>}
        {!own && !value && <option value="" disabled>{t('Choose a folder')}</option>}
        {folders.map((f) => <option key={f.device} value={f.device}>{f.device}</option>)}
      </select>
      <p className="text-xs text-muted-foreground">{t('Files it collected before are put in the Trash if you change this later; it then collects everything again into the new folder.')}</p>
    </div>
  )
}
