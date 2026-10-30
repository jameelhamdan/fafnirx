import type { ModuleFile } from '@/sdk'
import { downloadUrl, mediaUrl } from './api'
import type { Asset, FileInfo } from './types'

/** A photo or video of the library, as the modules are given it. */
export const fileOfAsset = (asset: Asset): ModuleFile => ({
  id: asset.id,
  name: asset.name,
  kind: asset.kind,
  src: mediaUrl(asset.id, 'original', asset.hash),
  poster: mediaUrl(asset.id, 'preview', asset.hash),
})

/** A file of the file browser, as the modules are given it. */
export const fileOfInfo = (info: FileInfo): ModuleFile => ({
  id: info.id,
  name: info.name,
  kind: info.mime.startsWith('video/') ? 'video' : info.mime.startsWith('image/') ? 'image' : null,
  src: downloadUrl(info.device, info.path, { inline: true }),
  poster: info.preview ? mediaUrl(info.id, 'preview') : undefined,
  deleted: info.deleted_at !== null,
})
