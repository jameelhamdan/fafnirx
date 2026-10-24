import { toast } from 'sonner'
import { useI18n } from '@/lib/i18n'
import VideoPlayer from '@/player/video-player'
import type { PlayerSource } from '@/player/types'

/**
 * The video player of the player module, joined to the app: its language and its messages. Every place that plays a
 * video (the viewer, the file details) uses this one, so they all look and behave the same.
 */
export default function AppVideo({ source, onFail, globalKeys, className }: {
  source: PlayerSource
  onFail: () => void
  globalKeys?: boolean
  className?: string
}) {
  const { t, lang } = useI18n()
  return (
    <VideoPlayer
      // A different video starts a new player, so nothing of the last one (the stream failing, the place) carries over.
      key={source.src}
      source={source}
      onFail={onFail}
      onNotice={(message) => toast.error(message)}
      t={t}
      lang={lang}
      storage="fafnirx"
      globalKeys={globalKeys}
      className={className}
    />
  )
}
