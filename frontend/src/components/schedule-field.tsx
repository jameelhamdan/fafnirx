import { SELECT } from '@/components/common'
import { Label } from '@/components/ui/label'
import { useT } from '@/lib/i18n'
import { SCHEDULES } from '@/lib/mirror-presets'

/** "How often" for something that runs on a schedule (a mirror, a source): the usual intervals, and the one it has already if that is another. */
export function ScheduleField({ id, hours, onChange }: { id: string; hours: string; onChange: (hours: string) => void }) {
  const t = useT()
  const options = SCHEDULES.some(([h]) => h === hours) ? SCHEDULES : [...SCHEDULES, [hours, `${hours} h`] as [string, string]]
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{t('How often')}</Label>
      <select id={id} className={`${SELECT} w-full`} value={hours} onChange={(e) => onChange(e.target.value)}>
        {options.map(([h, label]) => <option key={h} value={h}>{t(label)}</option>)}
      </select>
    </div>
  )
}
