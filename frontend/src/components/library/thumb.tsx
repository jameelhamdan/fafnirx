import type { ReactNode } from 'react'
import { usePicture } from '@/lib/pictures'
import { cn } from '@/lib/utils'

/**
 * A picture that is made when it is first looked at (see `lib/pictures`): an empty, pulsing place until it is there, and
 * `fallback` (an icon) when the master will not make one.
 */
export function Thumb({ src, className, fallback, fit = 'cover' }: { src: string; className?: string; fallback: ReactNode; fit?: 'cover' | 'contain' }) {
  const picture = usePicture(src)
  if (picture.state === 'none') return <div className={cn('grid place-items-center bg-muted/40', className)}>{fallback}</div>
  return (
    <img
      key={picture.attempt}
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      draggable={false}
      onLoad={picture.onLoad}
      onError={picture.onError}
      className={cn(fit === 'cover' ? 'object-cover' : 'object-contain', className, picture.state === 'pending' && 'animate-pulse bg-muted')}
    />
  )
}
