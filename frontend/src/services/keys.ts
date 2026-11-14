// Every query key of the app, so that a hook and the pages that refresh it name the same thing.
// A key is a prefix: invalidating ['files'] also refreshes ['files', 'places'] and every listing.

export const KEYS = {
  session: ['session'],
  settings: ['settings'],
  plugins: ['plugins'],
  dashboard: ['dashboard'],
  users: ['users'],
  devices: ['devices'],
  device: (key: string) => ['device', key],
  folders: ['folders'],
  folderPeople: ['folders', 'people'],
  files: ['files'],
  places: ['files', 'places'],
  info: ['info'],
  folder: (id: string) => ['folder', id],
  asset: (id: string) => ['asset', id],
  albums: ['albums'],
  album: ['album'],
  tags: ['tags'],
  trash: ['trash'],
  timeline: ['timeline'],
  sources: ['sources'],
  mirrors: ['mirrors'],
  mirrorScopes: ['mirrors', 'scopes'],
  queue: ['queue'],
}

const sets = (...keys: string[]) => keys.map((key) => [key])

/** Everything that shows library data: refreshed after any change to a photo, a video or a file. */
export const LIBRARY_KEYS = sets('timeline', 'asset', 'folder', 'albums', 'album', 'tags', 'trash', 'files', 'info')
/** What a renamed folder changes. */
export const RENAMED_KEYS = sets('devices', 'files', 'folders', 'albums', 'album', 'sources')
/** What an album of a folder changes. */
export const ALBUM_KEYS = sets('albums', 'album')
/** What a source writes into. */
export const SOURCE_KEYS = sets('sources', 'files')
