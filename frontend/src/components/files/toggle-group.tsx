import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

/** A small set of mutually exclusive icon buttons, such as list / grid. */
export function ToggleGroup<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T
  onChange: (v: T) => void
  options: { value: T; label: string; icon: LucideIcon }[]
}) {
  return (
    <div role="radiogroup" className="inline-flex rounded-md border bg-card p-0.5">
      {options.map(({ value: v, label, icon: Icon }) => (
        <button
          key={v}
          type="button"
          role="radio"
          aria-checked={value === v}
          aria-label={label}
          title={label}
          onClick={() => onChange(v)}
          className={cn('grid size-8 place-items-center rounded-sm text-muted-foreground transition-colors hover:text-foreground', value === v && 'bg-accent text-foreground')}
        >
          <Icon className="size-4" />
        </button>
      ))}
    </div>
  )
}
