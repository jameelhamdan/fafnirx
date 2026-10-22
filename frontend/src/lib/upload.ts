import { useSyncExternalStore } from 'react'
import { api, ApiError } from './api'

const CHUNK = 8 * 1024 * 1024
const PARALLEL = 3
const RETRIES = 4

export interface UploadItem {
  id: number
  device: string
  path: string
  size: number
  sent: number
  status: 'queued' | 'uploading' | 'done' | 'error'
  error?: string
}

type Source = { file: File; path: string }

let items: UploadItem[] = []
let sources = new Map<number, Source>()
let nextId = 1
let running = 0
const listeners = new Set<() => void>()
let onDone: () => void = () => {}

function emit() {
  items = [...items] // a new array so that subscribers see a change
  listeners.forEach((l) => l())
}

function patch(id: number, change: Partial<UploadItem>) {
  items = items.map((i) => (i.id === id ? { ...i, ...change } : i))
  listeners.forEach((l) => l())
}

/** Called when uploads finish, to refresh the file lists. */
export function setUploadsDoneHandler(fn: () => void) {
  onDone = fn
}

export function useUploads(): UploadItem[] {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => listeners.delete(l)
    },
    () => items,
  )
}

export function clearFinishedUploads() {
  items = items.filter((i) => i.status === 'queued' || i.status === 'uploading')
  emit()
}

/** Queues files for upload into `device`; `path` of each is relative to the device root. */
export function enqueueUploads(device: string, files: Source[]) {
  for (const f of files) {
    const id = nextId++
    sources.set(id, f)
    items.push({ id, device, path: f.path, size: f.file.size, sent: 0, status: 'queued' })
  }
  emit()
  pump()
}

function pump() {
  while (running < PARALLEL) {
    const next = items.find((i) => i.status === 'queued')
    if (!next) break
    running++
    patch(next.id, { status: 'uploading' })
    void send(next)
      .then(() => patch(next.id, { status: 'done', sent: next.size }))
      .catch((e: unknown) => patch(next.id, { status: 'error', error: e instanceof Error ? e.message : String(e) }))
      .finally(() => {
        running--
        sources.delete(next.id)
        if (running === 0 && !items.some((i) => i.status === 'queued')) onDone()
        pump()
      })
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function send(item: UploadItem) {
  const src = sources.get(item.id)!
  const started = await api.post<{ id: string }>('/api/uploads', {
    device: item.device,
    path: item.path,
    size: item.size,
    mtime_ms: src.file.lastModified,
  })
  const base = `/api/uploads/${started.id}`
  let offset = 0
  while (offset < item.size) {
    const end = Math.min(offset + CHUNK, item.size)
    let attempt = 0
    for (;;) {
      try {
        const res = await fetch(`${base}?offset=${offset}`, {
          method: 'PUT',
          credentials: 'same-origin',
          headers: { 'x-requested-with': 'fafnirx' },
          body: src.file.slice(offset, end),
        })
        if (res.status === 409) {
          // The server has a different amount than we think (an earlier try got through): continue from there.
          offset = ((await res.json()) as { received: number }).received
          break
        }
        if (!res.ok) throw new ApiError(res.status, ((await res.json().catch(() => ({}))) as { error?: string }).error ?? res.statusText)
        offset = end
        break
      } catch (e) {
        if ((e instanceof ApiError && e.status < 500) || ++attempt > RETRIES) throw e
        await sleep(500 * 2 ** attempt)
        // The chunk may have arrived even though the answer did not: ask where the server is.
        const status = await api.get<{ received: number }>(base).catch(() => null)
        if (status) offset = status.received
        break
      }
    }
    patch(item.id, { sent: offset })
  }
  await api.post(`${base}/finish`)
}

/** Files and folders dropped on the page, with their paths relative to what was dropped. */
export async function collectDropped(data: DataTransfer): Promise<Source[]> {
  const out: Source[] = []
  const walk = async (entry: FileSystemEntry, prefix: string): Promise<void> => {
    if (entry.isFile) {
      const file = await new Promise<File>((ok, fail) => (entry as FileSystemFileEntry).file(ok, fail))
      out.push({ file, path: prefix + entry.name })
    } else if (entry.isDirectory) {
      const reader = (entry as FileSystemDirectoryEntry).createReader()
      for (;;) {
        // readEntries returns a batch at a time until it returns nothing.
        const batch = await new Promise<FileSystemEntry[]>((ok, fail) => reader.readEntries(ok, fail))
        if (batch.length === 0) break
        for (const child of batch) await walk(child, `${prefix}${entry.name}/`)
      }
    }
  }
  const entries = [...data.items].map((i) => i.webkitGetAsEntry()).filter((e): e is FileSystemEntry => e !== null)
  for (const entry of entries) await walk(entry, '')
  return out
}

/** Files chosen with a file input; a folder input gives each file its relative path. */
export function fromInput(list: FileList): Source[] {
  return [...list].map((file) => ({ file, path: file.webkitRelativePath || file.name }))
}
