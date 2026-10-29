import type { ModuleFile } from '@/sdk'
import type { PlayerSource } from './player'

/** What the player is given for a file: the file itself, and the module's streaming addresses for it (see `crates/master/src/modules`). */
export function sourceOf(file: ModuleFile): PlayerSource {
  const hls = `/api/m/video/${file.id}/hls`
  return {
    src: file.src,
    poster: file.poster,
    title: file.name,
    // A deleted file is not streamed: it is only kept, as the file, for restoring.
    ...(file.deleted ? {} : { hls: `${hls}/master.m3u8`, subtitles: { list: `${hls}/subtitles.json`, track: `${hls}/subtitles/{index}.vtt` } }),
  }
}
