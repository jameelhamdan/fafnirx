import { msg, TaskCard, useT } from '@/sdk'

const KIND: Record<string, string> = {
  segment: msg('Video segments'),
  subtitles: msg('Subtitles'),
}

/** The video module's own work: segments being cut for a video that is playing, subtitle tracks being converted. */
export default function QueueSection({ report }: { report: unknown }) {
  const t = useT()
  return (
    <TaskCard
      title={t('Streaming')}
      summary={(slots, made) => t('Made as videos are watched, {slots} at a time. {made} made since it started.', { slots, made })}
      nothing={t('Nothing is being made now.')}
      kinds={KIND}
      report={report}
    />
  )
}
