import { Check, ChevronLeft, ChevronRight } from 'lucide-react'
import type { ReactNode } from 'react'
import { cx } from './util'

/** A line of a settings menu: a choice, with a tick when it is the current one. */
export function Choice({ on, onPick, children }: { on: boolean; onPick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={on}
      onClick={onPick}
      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm hover:bg-white/15 focus-visible:bg-white/15 focus-visible:outline-none"
    >
      <Check className={cx('size-4 shrink-0', !on && 'opacity-0')} />
      <span className="truncate">{children}</span>
    </button>
  )
}

/** A line that opens another page of the menu, and says what is chosen there. */
export function Door({ label, value, onOpen }: { label: string; value?: string; onOpen: () => void }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onOpen}
      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-sm hover:bg-white/15 focus-visible:bg-white/15 focus-visible:outline-none"
    >
      <span className="flex-1 truncate">{label}</span>
      {value && <span className="max-w-24 truncate text-xs text-white/60">{value}</span>}
      <ChevronRight className="size-4 shrink-0 text-white/60 rtl:rotate-180" />
    </button>
  )
}

export function Heading({ children }: { children: ReactNode }) {
  return <p className="px-2 pt-2 pb-0.5 text-[11px] font-medium tracking-wide text-white/60 uppercase first:pt-1">{children}</p>
}

/** The top of a page of the menu that was opened from another: a way back. */
export function Back({ label, onBack }: { label: string; onBack: () => void }) {
  return (
    <button
      type="button"
      onClick={onBack}
      className="mb-1 flex w-full items-center gap-1 rounded-md px-1 py-1.5 text-sm font-medium hover:bg-white/15 focus-visible:bg-white/15 focus-visible:outline-none"
    >
      <ChevronLeft className="size-4 rtl:rotate-180" />
      {label}
    </button>
  )
}

/** A named slider with its value on the right. */
export function Slider({ label, value, shown, min, max, step, onChange }: {
  label: string
  value: number
  shown: string
  min: number
  max: number
  step: number
  onChange: (value: number) => void
}) {
  return (
    <label className="block px-2 py-1">
      <span className="flex justify-between text-xs text-white/80">
        <span>{label}</span>
        <span className="tabular-nums">{shown}</span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="mt-1 h-1 w-full cursor-pointer accent-white"
      />
    </label>
  )
}

/** A few choices side by side, for a setting with a handful of values. */
export function Segments<T extends string>({ label, value, options, onChange }: {
  label: string
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
}) {
  return (
    <div className="px-2 py-1" role="radiogroup" aria-label={label}>
      <span className="text-xs text-white/80">{label}</span>
      <div className="mt-1 grid auto-cols-fr grid-flow-col gap-1">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={o.value === value}
            onClick={() => onChange(o.value)}
            className={cx(
              'rounded-md px-1.5 py-1 text-xs hover:bg-white/20 focus-visible:bg-white/20 focus-visible:outline-none',
              o.value === value ? 'bg-white/25 font-medium' : 'bg-white/10',
            )}
          >
            {o.label}
          </button>
        ))}
      </div>
    </div>
  )
}
