import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioCard, RadioGroup } from '@/components/ui/radio-group'
import { Textarea } from '@/components/ui/textarea'
import { useT } from '@/lib/i18n'
import { PRESETS } from '@/lib/mirror-presets'
import { presetOf, type Storage } from '@/lib/storage'
import { cn } from '@/lib/utils'

/** Picks a kind of storage and asks for what it needs. */
export function StorageForm({ value, onChange }: { value: Storage; onChange: (next: Storage) => void }) {
  const t = useT()
  const preset = presetOf(value)
  const set = (patch: Partial<Storage>) => onChange({ ...value, ...patch })
  return (
    <div className="space-y-3">
      <RadioGroup aria-label={t('Storage')} className="sm:grid-cols-3" value={value.preset} onValueChange={(preset) => set({ preset, values: {} })}>
        {PRESETS.map((p) => <RadioCard key={p.id} value={p.id}>{t(p.label)}</RadioCard>)}
      </RadioGroup>
      <p className="text-xs text-muted-foreground">{t(preset.hint)}</p>
      <div className="grid gap-3 sm:grid-cols-2">
        {preset.fields.map((f) => (
          <div key={f.key} className="space-y-1.5">
            <Label htmlFor={`st-${f.key}`}>{t(f.label)}{f.optional && <span className="text-muted-foreground"> · {t('optional')}</span>}</Label>
            <Input id={`st-${f.key}`} type={f.secret ? 'password' : 'text'} autoComplete="off" placeholder={f.placeholder} value={value.values[f.key] ?? ''} onChange={(e) => set({ values: { ...value.values, [f.key]: e.target.value } })} />
          </div>
        ))}
        <div className={cn('space-y-1.5', preset.fields.length % 2 === 0 && 'sm:col-span-2')}>
          <Label htmlFor="st-path">{t(preset.pathLabel)}</Label>
          <Input id="st-path" placeholder={preset.pathPlaceholder} value={value.path} onChange={(e) => set({ path: e.target.value })} />
        </div>
      </div>
      <Collapsible className="text-sm">
        <CollapsibleTrigger className="text-muted-foreground hover:text-foreground">{t('More backend options')}</CollapsibleTrigger>
        <CollapsibleContent>
          <Textarea rows={3} className="mt-2 font-mono text-xs" placeholder="key=value" value={value.advanced} onChange={(e) => set({ advanced: e.target.value })} />
          <p className="mt-1 text-xs text-muted-foreground">{t('One key=value per line, as rclone names them. Stored on the master and never shown again.')}</p>
        </CollapsibleContent>
      </Collapsible>
    </div>
  )
}
