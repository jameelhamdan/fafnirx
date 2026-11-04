import { useQuery } from '@tanstack/react-query'
import { SELECT } from '@/components/common'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api } from '@/lib/api'
import { useT } from '@/lib/i18n'
import type { Listing } from '@/lib/types'

/** A folder and a subfolder of it: where a source writes. `null` is a new folder of its own. */
export interface Place {
  device: string
  path: string
}

/**
 * Picks the folder a source writes into: one of your folders, a shared folder you may change, or the master files, and any
 * subfolder of it (made when the first file arrives). `own` offers a new folder of the source's own (when it is added).
 */
export function FolderPlace({ value, onChange, own }: { value: Place | null; onChange: (next: Place | null) => void; own?: boolean }) {
  const t = useT()
  const places = useQuery({ queryKey: ['files', 'places'], queryFn: () => api.get<Listing>('/api/files') })
  const folders = (places.data?.entries ?? []).filter((e) => e.kind === 'device' && e.writable && ['local', 'master'].includes(e.device_kind ?? ''))
  const device = value?.device ?? ''
  return (
    <div className="space-y-1.5">
      <Label htmlFor="place-device">{t('Where the files go')}</Label>
      <div className="grid gap-2 sm:grid-cols-2">
        <select
          id="place-device"
          className={`${SELECT} w-full`}
          value={device}
          onChange={(e) => onChange(e.target.value ? { device: e.target.value, path: value?.path ?? '' } : null)}
        >
          {own && <option value="">{t('A new folder of its own')}</option>}
          {!own && !device && <option value="" disabled>{t('Choose a folder')}</option>}
          {folders.map((f) => <option key={f.device} value={f.device}>{f.device === '_master' ? t('Master files') : f.device}</option>)}
        </select>
        <Input
          aria-label={t('Subfolder')}
          placeholder={t('Subfolder, e.g. Shows/Documentaries')}
          disabled={!value}
          value={value?.path ?? ''}
          onChange={(e) => value && onChange({ ...value, path: e.target.value })}
        />
      </div>
      <p className="text-xs text-muted-foreground">{t('Files it collected before stay where they are if you change this later; it then collects everything again into the new place.')}</p>
    </div>
  )
}
