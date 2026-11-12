// Shapes of the master's web API. The responses that are Rust structs in crates/master are generated from them into ./generated
// (`cargo test -p fafnirx-master` writes them; see .cargo/config.toml) and only re-exported here, so they cannot drift. What
// is written by hand below is what the master does not have as a struct: plugin and module descriptions, and the small unions.
import type { Session as GeneratedSession } from './generated/Session'

export type { Activity } from './generated/Activity'
export type { AlbumInfo } from './generated/AlbumInfo'
export type { Asset } from './generated/Asset'
export type { AssetDetail } from './generated/AssetDetail'
export type { Buckets } from './generated/Buckets'
export type { Dashboard } from './generated/Dashboard'
export type { DeviceDetail } from './generated/DeviceDetail'
export type { DeviceSummary } from './generated/DeviceSummary'
export type { Entry } from './generated/Entry'
export type { FileInfo } from './generated/FileInfo'
export type { Folder } from './generated/Folder'
export type { FolderState } from './generated/FolderState'
export type { JobState } from './generated/JobState'
export type { Listing } from './generated/Listing'
export type { QueueJob } from './generated/QueueJob'
export type { QueueState } from './generated/QueueState'
export type { Recent } from './generated/Recent'
export type { Settings } from './generated/Settings'
export type { TagInfo } from './generated/TagInfo'
export type { Trash } from './generated/Trash'
export type { TrashEntry } from './generated/TrashEntry'
export type { UserInfo } from './generated/UserInfo'


export type Role = 'admin' | 'normal' | 'readonly'

export type Scope = 'timeline' | 'favorites' | 'archive'

/** A plugin as the administrator's page shows it. */
export interface PluginState {
  id: string
  version: string | null
  /** `up`, `setup` (running, but `problem` says what it is missing), `down` (not reached, `problem` says why) or `missing` (required by `FAFNIRX_PLUGINS`, never connected). */
  state: 'up' | 'setup' | 'down' | 'missing'
  problem: string | null
  /** Listed in the master's `FAFNIRX_PLUGINS`: it must be running. */
  expected: boolean
}

/** What the master says a module is (its manifest). `ui` is there when the module has a part of the web app to load. */
export interface ModuleInfo {
  id: string
  version: string
  contract: number
  capabilities: string[]
  ui?: { entry: string; css?: string; sdk: number; strings?: string[] }
}

/** The session, with the manifests of the modules that are up typed. */
export type Session = Omit<GeneratedSession, 'modules'> & { modules?: Record<string, ModuleInfo> }
