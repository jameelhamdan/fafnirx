import { cn } from '@/lib/utils'

/** Plays the original in the browser's own player: what a video gets when the video module is not up. */
export function NativeVideo({ src, poster, onFail, className }: { src: string; poster?: string; onFail: () => void; className?: string }) {
  return <video key={src} src={src} poster={poster} controls autoPlay playsInline preload="metadata" onError={onFail} className={cn('max-h-full max-w-full bg-black', className)} />
}
