// Shapes of the master's admin API (`web`, `devices`, `files`, ... in crates/master/src).

export type DeviceKind = 'device' | 'local' | 'source'

export interface Activity {
  phase: string
  file: string
  files_done: number
  files_total: number
  bytes_done: number
  bytes_total: number
  speed_bps: number
}

export interface DeviceSummary {
  name: string
  kind: DeviceKind
  online: boolean
  revoked: boolean
  created_at: number
  hostname: string | null
  os: string | null
  ip: string | null
  client_version: string | null
  disk_total: number | null
  disk_free: number | null
  last_seen: number | null
  last_collected: number | null
  bytes: number
  files: number
  activity: Activity | null
}

export interface Recent {
  id: number
  ts: number
  action: string
  path: string
  detail: string | null
  device: string
}

export interface DeviceDetail {
  device: DeviceSummary
  events: Recent[]
  token: string | null
}

export interface JobState {
  name: string
  status: string
  running: boolean
  enabled: boolean
  interval_secs: number
  last_run_at: number | null
  last_ok_at: number | null
  message: string
  detail: Record<string, unknown>
}

export interface Alert {
  id: number
  kind: string
  key: string
  message: string
  created_at: number
  updated_at: number | null
}

export interface Dashboard {
  devices: DeviceSummary[]
  online: number
  totals: { files: number; bytes: number; marked_deleted: number }
  activity: { day: number; added: number; updated: number; deleted: number }[]
  recent: Recent[]
  sources: JobState[]
  mirrors: JobState[]
  alerts: Alert[]
  disk: { total: number; used: number; free: number; percent: number } | null
}

export interface Entry {
  kind: 'device' | 'dir' | 'file'
  name: string
  device: string
  path: string
  size: number | null
  mtime: number | null
  changed: number | null
  files: number | null
  versions: number | null
  deleted_at: number | null
  purge_at: number | null
  mime: string | null
  preview: boolean
  width: number | null
  height: number | null
  taken_at: number | null
}

export interface Listing {
  device: string | null
  path: string
  entries: Entry[]
  total: number
  searching: boolean
  retention_days: number
}

export interface VersionInfo {
  id: number
  created_at: number
  size: number
  mtime: number
  hash: string
  current: boolean
  pruned_at: number | null
  available: boolean
}

export interface FileInfo {
  device: string
  path: string
  name: string
  deleted_at: number | null
  purge_at: number | null
  size: number
  mtime: number
  changed: number
  hash: string
  mime: string
  preview: boolean
  width: number | null
  height: number | null
  taken_at: number | null
  exif: Record<string, string> | null
  versions: VersionInfo[]
  events: Recent[]
}

export interface Settings {
  version_count: number
  retention_days: number
}

export interface FileRef {
  device: string
  path: string
}

export interface Session {
  admin: boolean
  /** Master version; only sent to a logged-in admin. */
  version?: string
  /** Tile address of the map, with {z}, {x} and {y} in it. */
  map_tiles?: string
}

/** A photo or video of the library. */
export interface Asset {
  id: number
  device: string
  path: string
  name: string
  kind: 'image' | 'video'
  size: number
  /** Start of the content hash: part of picture links so that they never go stale. */
  hash: string
  width: number | null
  height: number | null
  duration_ms: number | null
  /** Unix seconds; the camera's date, a corrected one, or the file's own. Read as UTC. */
  when: number
  favorite: boolean
  archived: boolean
  rating: number | null
  lat: number | null
  lon: number | null
}

export type Scope = 'timeline' | 'favorites' | 'archive'

export interface Buckets {
  /** `days` is how many files each day holds, newest day first. */
  months: { month: string; count: number; days: number[] }[]
  total: number
}

export interface AssetDetail extends Asset {
  mime: string
  description: string
  camera_when: number | null
  camera_lat: number | null
  camera_lon: number | null
  when_edited: boolean
  place_edited: boolean
  exif: Record<string, string> | null
  codec: string | null
  tags: { id: number; name: string }[]
  albums: { id: number; name: string }[]
  changed: number
  preview: boolean
}

export interface TagInfo {
  id: number
  name: string
  count: number
}

export interface AlbumInfo {
  id: number
  name: string
  description: string
  created_at: number
  updated_at: number
  count: number
  bytes: number
  cover: number | null
}

export interface TrashEntry {
  id: number
  device: string
  path: string
  name: string
  size: number
  deleted_at: number
  purge_at: number
  version: number
  kind: 'image' | 'video' | null
  hash: string
}

export interface Trash {
  entries: TrashEntry[]
  total: number
  retention_days: number
}

export interface MapData {
  /** [id, latitude, longitude, when] */
  points: [number, number, number, number][]
  truncated: boolean
}
