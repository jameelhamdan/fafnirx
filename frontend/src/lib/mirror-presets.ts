import { msg } from '@/lib/i18n'

/** One input of a storage preset. */
export interface PresetField {
  key: string
  label: string
  placeholder?: string
  /** Shown as dots; stored on the master and never shown again. */
  secret?: boolean
  /** rclone wants this password in its own scrambled form: the master does that before storing it. */
  obscure?: boolean
  optional?: boolean
}

/** Where a mirror puts its files: an rclone backend and the few things it needs. */
export interface Preset {
  id: string
  /** The rclone backend; empty for a remote of the user's own rclone.conf. */
  type: string
  label: string
  hint: string
  pathLabel: string
  pathPlaceholder: string
  /** The folder may be left empty (the bucket or the whole storage). */
  pathOptional?: boolean
  /** Only offered for a source: a mirror cannot write there. */
  sourceOnly?: boolean
  fields: PresetField[]
}

/** How often a mirror runs or a source is read: hours and the label. */
export const SCHEDULES: [string, string][] = [['1', msg('Every hour')], ['6', msg('Every 6 hours')], ['24', msg('Every day')], ['72', msg('Every 3 days')], ['168', msg('Every week')]]

export const PRESETS: Preset[] = [
  {
    id: 'b2',
    type: 'b2',
    label: msg('Backblaze B2'),
    hint: msg('Cheap object storage. Use an application key limited to one bucket.'),
    pathLabel: msg('Bucket and folder'),
    pathPlaceholder: 'my-bucket/fafnirx',
    pathOptional: true,
    fields: [
      { key: 'account', label: msg('Key ID'), placeholder: '0012345abcde' },
      { key: 'key', label: msg('Application key'), secret: true },
    ],
  },
  {
    id: 's3',
    type: 's3',
    label: msg('S3-compatible'),
    hint: msg('Amazon S3, Wasabi, Cloudflare R2, MinIO and others.'),
    pathLabel: msg('Bucket and folder'),
    pathPlaceholder: 'my-bucket/fafnirx',
    pathOptional: true,
    fields: [
      { key: 'provider', label: msg('Provider'), placeholder: 'AWS, Wasabi, Cloudflare, Minio, Other' },
      { key: 'endpoint', label: msg('Endpoint'), placeholder: 'https://…', optional: true },
      { key: 'region', label: msg('Region'), placeholder: 'eu-west-1', optional: true },
      { key: 'access_key_id', label: msg('Access key ID') },
      { key: 'secret_access_key', label: msg('Secret access key'), secret: true },
    ],
  },
  {
    id: 'sftp',
    type: 'sftp',
    label: msg('SFTP / SSH'),
    hint: msg('Any server or NAS you can log in to over SSH.'),
    pathLabel: msg('Folder on the server'),
    pathPlaceholder: '/backups/fafnirx',
    fields: [
      { key: 'host', label: msg('Host'), placeholder: 'nas.example.com' },
      { key: 'port', label: msg('Port'), placeholder: '22', optional: true },
      { key: 'user', label: msg('User') },
      { key: 'pass', label: msg('Password'), secret: true, obscure: true },
    ],
  },
  {
    id: 'webdav',
    type: 'webdav',
    label: msg('WebDAV / Nextcloud'),
    hint: msg('Nextcloud, ownCloud and other WebDAV servers.'),
    pathLabel: msg('Folder'),
    pathPlaceholder: 'fafnirx',
    fields: [
      { key: 'url', label: msg('WebDAV URL'), placeholder: 'https://cloud.example.com/remote.php/dav/files/me' },
      { key: 'vendor', label: msg('Server'), placeholder: 'nextcloud, owncloud or other', optional: true },
      { key: 'user', label: msg('User') },
      { key: 'pass', label: msg('Password or app password'), secret: true, obscure: true },
    ],
  },
  {
    id: 'drive',
    type: 'drive',
    label: msg('Google Drive'),
    hint: msg('Folders shared with a service account: paste its JSON key and the id of a shared folder.'),
    pathLabel: msg('Folder'),
    pathPlaceholder: 'Photos',
    pathOptional: true,
    sourceOnly: true,
    fields: [
      { key: 'service_account_credentials', label: msg('Service account key (JSON)'), secret: true },
      { key: 'root_folder_id', label: msg('Folder id') },
    ],
  },
  {
    id: 'local',
    type: 'local',
    label: msg('Folder or drive'),
    hint: msg('A folder on the master, such as a mounted external disk or network share.'),
    pathLabel: msg('Folder'),
    pathPlaceholder: '/mnt/offsite',
    fields: [],
  },
  {
    id: 'conf',
    type: '',
    label: msg('Remote from rclone.conf'),
    hint: msg('Use a remote you already set up in the rclone config file (OneDrive, Dropbox, …).'),
    pathLabel: msg('Remote and folder'),
    pathPlaceholder: 'onedrive:fafnirx',
    fields: [],
  },
]
