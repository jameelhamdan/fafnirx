import { Cloud, File, FileArchive, FileAudio, FileImage, FileText, FileVideo, Folder, HardDrive, Monitor, Server } from 'lucide-react'
import type { Entry } from '@/lib/types'
import { cn } from '@/lib/utils'

/** The icon of a device entry says what it is: the master files, a computer that sends, a local folder or a source. */
function DeviceIcon({ kind, className }: { kind: Entry['device_kind']; className: string }) {
  if (kind === 'master') return <Server className={cn(className, 'text-primary')} />
  if (kind === 'local') return <HardDrive className={cn(className, 'text-warning')} />
  if (kind === 'source') return <Cloud className={cn(className, 'text-muted-foreground')} />
  return <Monitor className={cn(className, 'text-success')} />
}

export function FileIcon({ entry, className }: { entry: Pick<Entry, 'kind' | 'mime'> & { device_kind?: Entry['device_kind'] }; className?: string }) {
  const cls = cn('size-5 shrink-0', className)
  if (entry.kind === 'device') return <DeviceIcon kind={entry.device_kind ?? null} className={cls} />
  if (entry.kind === 'dir') return <Folder className={cn(cls, 'fill-primary/15 text-primary')} />
  const mime = entry.mime ?? ''
  if (mime.startsWith('image/')) return <FileImage className={cn(cls, 'text-success')} />
  if (mime.startsWith('video/')) return <FileVideo className={cn(cls, 'text-warning')} />
  if (mime.startsWith('audio/')) return <FileAudio className={cn(cls, 'text-warning')} />
  if (mime.startsWith('text/') || mime === 'application/pdf') return <FileText className={cn(cls, 'text-muted-foreground')} />
  if (/zip|tar|rar|7z|compress/.test(mime)) return <FileArchive className={cn(cls, 'text-muted-foreground')} />
  return <File className={cn(cls, 'text-muted-foreground')} />
}
