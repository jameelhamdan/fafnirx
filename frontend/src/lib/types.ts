// Shapes of the master's web API (`web`, `devices`, `files`, ... in crates/master/src).

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
  /** The name of the device; unique among the devices of its user. */
  name: string
  /** The user the device belongs to. */
  user: string
  /** `<user>/<name>`: how the files, the library and the live updates name the device. */
  folder: string
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
  /** What the API calls it, and what is shown: user/name for a source, the name for a mirror. */
  key: string
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
  id: string
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
  activity: { day: number; added: number; updated: number; renamed: number; deleted: number }[]
  recent: Recent[]
  sources: JobState[]
  mirrors: JobState[]
  alerts: Alert[]
  disk: { total: number; used: number; free: number; percent: number } | null
}

export interface Entry {
  /** A photo or video whose details are not read yet: it is still being processed. */
  processing: boolean
  kind: 'device' | 'dir' | 'file'
  /** For a file: its id in the library (the address of its viewer). */
  id: string | null
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
  /** For a device entry: what it is. */
  device_kind: DeviceKind | "master" | null
  /** For a device entry: whether the web app may add, change or delete files there. */
  writable: boolean
}

export interface Listing {
  device: string | null
  /** What the device is and whether the web app may change its files; null at the top level. */
  device_kind: DeviceKind | "master" | null
  writable: boolean
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
  /** Work still to do on a photo or video: waiting, running or failed. */
  work: 'waiting' | 'running' | 'failed' | null
  /** The id of the file, also its address for streaming. */
  id: string
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

export type Role = 'admin' | 'normal' | 'readonly'

export interface UserInfo {
  name: string
  role: Role
  disabled: boolean
  created_at: number
  devices: number
}

export interface Session {
  /** Who is logged in; null when nobody is. */
  user: { name: string; role: Role } | null
  /** Master version; only sent to a logged-in user. */
  version?: string
  /** Tile address of the map, with {z}, {x} and {y} in it. */
  map_tiles?: string
  /** Videos can be streamed in several qualities (a streaming service is set up). */
  stream?: boolean
}

/** A photo or video of the library. */
export interface Asset {
  /** Its details are not read yet (see Processing). */
  processing: boolean
  id: string
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
  work: 'waiting' | 'running' | 'failed' | null
  mime: string
  description: string
  file_description: string | null
  file_tags: string[]
  camera_when: number | null
  camera_lat: number | null
  camera_lon: number | null
  when_edited: boolean
  place_edited: boolean
  exif: Record<string, string> | null
  codec: string | null
  tags: { id: string; name: string }[]
  albums: { id: string; name: string }[]
  changed: number
  preview: boolean
  /** What the .nfo next to a video says (Kodi/Jellyfin naming). */
  described: Described | null
}

export interface Described {
  title: string | null
  show: string | null
  season: number | null
  episode: number | null
  plot: string | null
  aired: string | null
}

export interface QueueJob {
  /** What a finished job found, such as the picture a lookup chose. */
  note: string | null
  id: string
  kind: string
  status: 'queued' | 'running' | 'failed' | 'done'
  device: string
  path: string
  attempts: number
  error: string | null
  created_at: number
  started_at: number | null
  finished_at: number | null
  retry_at: number | null
}

export interface QueueState {
  paused: boolean
  ffmpeg: boolean
  can_control: boolean
  summary: { kind: string; queued: number; running: number; failed: number; done: number }[]
  jobs: QueueJob[]
  stream: {
    slots: number
    made: number
    active: { kind: string; video: string; running: boolean; since: number }[]
    failed: { kind: string; video: string; error: string; at: number }[]
  } | null
}

export interface TagInfo {
  id: string
  name: string
  count: number
}

export interface AlbumInfo {
  id: string
  name: string
  description: string
  created_at: number
  updated_at: number
  count: number
  bytes: number
  cover: string | null
  /** Set when the album shows a folder of a device: what is in the folder is in the album. */
  folder: { device: string; path: string } | null
}

export interface TrashEntry {
  id: string
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
  points: [string, number, number, number][]
  truncated: boolean
}
