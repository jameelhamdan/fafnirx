import type { PlayerSource } from '@/player/types'
import { downloadUrl, mediaUrl, thumbUrl } from './api'
import type { Asset, FileInfo } from './types'

/** The streaming addresses of a file: the master serves them for the file's id (see `library::stream` in the master). */
function streamOf(id: number): Pick<PlayerSource, 'hls' | 'subtitles'> {
  const base = `/api/stream/${id}`
  return { hls: `${base}/master.m3u8`, subtitles: { list: `${base}/subtitles.json`, track: `${base}/subtitles/{index}.vtt` } }
}

/** A video of the library. `stream`: whether the master has a streaming service. */
export const assetSource = (asset: Asset, stream: boolean): PlayerSource => ({
  src: mediaUrl(asset.id, 'video', asset.hash),
  poster: mediaUrl(asset.id, 'preview', asset.hash),
  title: asset.name,
  ...(stream ? streamOf(asset.id) : {}),
})

/** A video in the file browser. A deleted file is not streamed: it is only kept, as the file, for restoring. */
export const fileSource = (info: FileInfo, stream: boolean): PlayerSource => ({
  src: downloadUrl(info.device, info.path, { inline: true }),
  poster: info.preview ? thumbUrl(info.device, info.path, 1000) : undefined,
  title: info.name,
  ...(stream && info.deleted_at === null ? streamOf(info.id) : {}),
})
