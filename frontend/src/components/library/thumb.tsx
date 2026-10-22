import { useState, type ReactNode } from 'react'
import { cn } from '@/lib/utils'

/** A picture that is drawn lazily, and gives way to `fallback` (an icon) when the master cannot make one. */
export function Thumb({ src, className, fallback, fit = 'cover' }: { src: string; className?: string; fallback: ReactNode; fit?: 'cover' | 'contain' }) {
  const [failed, setFailed] = useState<string | null>(null)
  if (failed === src) return <div className={cn('grid place-items-center bg-muted/40', className)}>{fallback}</div>
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      draggable={false}
      onError={() => setFailed(src)}
      className={cn(fit === 'cover' ? 'object-cover' : 'object-contain', className)}
    />
  )
}
