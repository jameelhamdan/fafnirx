/** Subtitle files bigger than this are not subtitles. */
export const MAX_SUBTITLE_BYTES = 4 << 20

/**
 * WebVTT for a subtitle file the person picked: SubRip (`.srt`) is converted, WebVTT is taken as it is. Anything else
 * (styled SSA/ASS, pictures) is for the server to convert, so it gives `null`.
 */
export function toVtt(text: string, fileName: string): string | null {
  const body = text.replace(/^﻿/, '').replace(/\r\n?/g, '\n')
  const extension = fileName.toLowerCase().split('.').pop()
  if (extension === 'vtt') return body.startsWith('WEBVTT') ? body : null
  if (extension === 'srt') return `WEBVTT\n\n${body.replace(/(\d{1,2}:\d{2}:\d{2}),(\d{3})/g, '$1.$2')}`
  return null
}
