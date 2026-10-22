import { File, FileArchive, FileAudio, FileImage, FileText, FileVideo, Folder, Monitor } from 'lucide-react'
import type { Entry } from '@/lib/types'
import { cn } from '@/lib/utils'

export function FileIcon({ entry, className }: { entry: Pick<Entry, 'kind' | 'mime'>; className?: string }) {
  const cls = cn('size-5 shrink-0', className)
  if (entry.kind === 'device') return <Monitor className={cn(cls, 'text-primary')} />
  if (entry.kind === 'dir') return <Folder className={cn(cls, 'fill-primary/15 text-primary')} />
  const mime = entry.mime ?? ''
  if (mime.startsWith('image/')) return <FileImage className={cn(cls, 'text-success')} />
  if (mime.startsWith('video/')) return <FileVideo className={cn(cls, 'text-warning')} />
  if (mime.startsWith('audio/')) return <FileAudio className={cn(cls, 'text-warning')} />
  if (mime.startsWith('text/') || mime === 'application/pdf') return <FileText className={cn(cls, 'text-muted-foreground')} />
  if (/zip|tar|rar|7z|compress/.test(mime)) return <FileArchive className={cn(cls, 'text-muted-foreground')} />
  return <File className={cn(cls, 'text-muted-foreground')} />
}

