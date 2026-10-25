import type { Entry } from './types'

/** Identifies an entry of a listing, for selection. */
export const entryKey = (e: Entry) => `${e.kind}|${e.device}|${e.path}`

/** Whether the library lists a file of this name (a photo or a video): the same extensions as the master's `files.media`. */
export const isLibraryFile = (name: string) =>
  /\.(jpg|jpeg|png|gif|webp|bmp|tif|tiff|heic|heif|avif|mp4|mov|mkv|avi|webm|m4v|wmv|mts|3gp|mpg|mpeg)$/i.test(name)
