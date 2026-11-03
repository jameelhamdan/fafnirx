import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { AcknowledgeField, FileLimitsFields, useT } from '@/sdk'
import type { Web } from './web'

/** What a web source needs: addresses of files and/or a feed, and what to keep of them. */
export function WebForm({ value, onChange }: { value: Web; onChange: (next: Web) => void }) {
  const t = useT()
  const set = (patch: Partial<Web>) => onChange({ ...value, ...patch })
  return (
    <div className="space-y-3">
      <div className="space-y-1.5">
        <Label htmlFor="wb-urls">{t('Addresses')}</Label>
        <Textarea id="wb-urls" rows={4} className="font-mono text-xs" placeholder={'https://example.org/files/report.pdf'} value={value.urls} onChange={(e) => set({ urls: e.target.value })} />
        <p className="text-xs text-muted-foreground">{t('One web address of a file per line.')}</p>
      </div>
      <div className="space-y-1.5">
        <Label htmlFor="wb-feed">{t('Feed (optional)')}</Label>
        <Input id="wb-feed" type="url" autoComplete="off" placeholder="https://example.org/feed.xml" value={value.feed} onChange={(e) => set({ feed: e.target.value })} />
        <p className="text-xs text-muted-foreground">{t('An RSS or Atom feed whose entries link to files, read on every run. A feed only adds files: what it stops listing is not deleted.')}</p>
      </div>
      <FileLimitsFields idPrefix="wb" value={value} onChange={set} placeholder={'\\.(pdf|zip)$'} />
      <AcknowledgeField checked={value.acknowledged} onChange={(acknowledged) => set({ acknowledged })}>
        {t('I have the right to copy what this source downloads.')}
      </AcknowledgeField>
    </div>
  )
}
