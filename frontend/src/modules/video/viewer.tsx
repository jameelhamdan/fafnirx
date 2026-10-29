import { toast, useI18n, type ViewerProps } from '@/sdk'
import { VideoPlayer } from './player'
import { sourceOf } from './source'

/**
 * The video player of the video module, joined to the app: its language and its messages. Every place that plays a
 * video (the viewer, the file details) uses this one, so they all look and behave the same.
 */
export default function VideoViewer({ file, onFail, globalKeys, className }: ViewerProps) {
  const { t, lang } = useI18n()
  return (
    <VideoPlayer
      // A different video starts a new player, so nothing of the last one (the stream failing, the place) carries over.
      key={file.src}
      source={sourceOf(file)}
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
