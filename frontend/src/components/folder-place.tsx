import { useQuery } from '@tanstack/react-query'
import { Picker } from '@/components/common'
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
  const folders = (places.data?.entries ?? []).filter((e) => e.kind === 'folder' && e.writable && ['user', 'master'].includes(e.folder_kind ?? ''))
  return (
    <div className="space-y-1.5">
      <Label htmlFor="place-folder">{t('Where the files go')}</Label>
      <Picker
        id="place-folder"
        className="w-full"
        value={value}
        onChange={onChange}
        placeholder={t('Choose a folder')}
        options={[...(own ? [['', t('A new folder of its own')] as [string, string]] : []), ...folders.map((f): [string, string] => [f.folder, f.folder])]}
      />
      <p className="text-xs text-muted-foreground">{t('Files it collected before are put in the Trash if you change this later; it then collects everything again into the new folder.')}</p>
    </div>
  )
}
