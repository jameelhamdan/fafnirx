// End-to-end test for sources (any rclone storage) and mirrors (replicas, encrypted backups, renames).
//
// The sources and the mirror targets are local directories through rclone's `local` backend. Everything else (the
// master, the client) is the real thing. Needs rclone on PATH or in $FAFNIRX_RCLONE.
//   cd tests && npm install && node e2e-io.mjs
import { randomBytes } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { CHUNK, check, enrollDevice, harness, passedChecks, sleep, until } from './lib.mjs'

const rclone = (() => {
  if (process.env.FAFNIRX_RCLONE) return process.env.FAFNIRX_RCLONE
  try {
    return execFileSync(process.platform === 'win32' ? 'where' : 'which', ['rclone'], { encoding: 'utf8' }).split(/\r?\n/)[0]
  } catch {
    return null
  }
})()
if (!rclone) {
  console.error('rclone not found; install it or set FAFNIRX_RCLONE')
  process.exit(1)
}

/** Every file under `root` as relative path -> bytes. */
function tree(root) {
  const out = {}
  const walk = (dir) => {
    for (const e of fs.existsSync(dir) ? fs.readdirSync(dir, { withFileTypes: true }) : []) {
      const p = path.join(dir, e.name)
      if (e.isDirectory()) walk(p)
      else out[path.relative(root, p).replaceAll('\\', '/')] = fs.readFileSync(p)
    }
  }
  walk(root)
  return out
}

const h = await harness('fafnirx-e2e-io-', {
  env: { FAFNIRX_RCLONE: rclone },
})
const { tmp, data, schema, master } = h

const status = async (table, name) => (await schema.rows(`SELECT last_status, last_message, last_run_at FROM ${table} WHERE name = $1`, [name]))[0]
const finished = async (table, name) => {
  const s = await status(table, name)
  return s && s[0] !== 'running' && s[0] !== null ? s : null
}
let admin
async function runAndWait(table, name) {
  const before = (await status(table, name))?.[2] ?? 0
  await sleep(1050) // run stamps have one second resolution
  await admin.post(`/api/${table}/${table === 'sources' ? 'admin/' : ''}${name}/run`)
  const row = await until(async () => {
    const s = await status(table, name)
    return s?.[2] > before && s[0] !== 'running' ? s : null
  }, 90)
  if (!row) check(false, `${table} ${name} finished its run`)
  return row
}
const livePaths = async (device) =>
  new Set((await schema.rows("SELECT f.path FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = $1 AND f.deleted_at IS NULL", [device])).map((r) => r[0]))
const stored = (device, p) => fs.readFileSync(path.join(data, 'files', ...(device === '_master' ? [] : ['users']), device, p))
const openAlerts = async () => {
  const r = await admin.get('/api/dashboard')
  if (!r.json?.alerts) throw new Error(`the dashboard failed: ${r.status} ${JSON.stringify(r.json)}`)
  return r.json.alerts
}

try {
  await master.start()
  admin = await master.admin()

  // --- a source of any rclone storage (here a plain directory) -----------------------------------------------------------------------------
  const big = randomBytes(CHUNK * 2 + 777)
  const photos = path.join(tmp, 'photos')
  const put = (name, data) => {
    fs.mkdirSync(path.dirname(path.join(photos, name)), { recursive: true })
    fs.writeFileSync(path.join(photos, name), data)
  }
  put('a.txt', 'first version')
  put('big.bin', big)
  put('dup.txt', 'one')
  put('sub/b.txt', 'in a sub folder')

  const added = await admin.post('/api/sources', { name: 'photos', remote_type: 'local', remote_path: photos.replaceAll('\\', '/'), interval_hours: 24 })
  check(added.status === 200, `a source is added through the web API (${added.json?.error ?? ''})`)
  const first = await until(() => finished('sources', 'photos'), 90)
  check(first?.[0] === 'ok', `the first run succeeds (${first?.[0]}: ${JSON.stringify(first?.[1])})`)

  let paths = await livePaths('admin/source-photos')
  const want = ['a.txt', 'big.bin', 'dup.txt', 'sub/b.txt']
  check(paths.size === want.length && want.every((p) => paths.has(p)), `the source holds the files of the folder (${[...paths].sort()})`)
  check(stored('admin/source-photos', 'big.bin').equals(big), 'a multi-chunk file is stored byte for byte under files/<source>/')
  const devices = (await admin.get('/api/devices')).json
  check(devices.filter((d) => d.kind === 'device').length === 0 && devices.some((d) => d.kind === 'source'), 'a source is listed as a source, never as a device')

  // --- changes in the folder: edit, delete, rename, add --------------------------------------------------------------------------------
  put('a.txt', 'second version')
  fs.rmSync(path.join(photos, 'sub', 'b.txt'))
  fs.renameSync(path.join(photos, 'big.bin'), path.join(photos, 'big-renamed.bin'))
  put('new.txt', 'fresh')
  const second = await runAndWait('sources', 'photos')
  check(second[0] === 'ok', `the second run succeeds (${JSON.stringify(second[1])})`)
  paths = await livePaths('admin/source-photos')
  check(paths.has('big-renamed.bin') && !paths.has('big.bin') && paths.has('new.txt') && !paths.has('sub/b.txt'), 'edit, rename, add and delete are all picked up')
  check(stored('admin/source-photos', 'a.txt').toString() === 'second version', 'an edited file has the new content')
  const info = (await admin.get('/api/files/info?device=admin/source-photos&path=a.txt')).json
  check(info.versions.length === 2, 'the previous version is kept')
  const gone = await admin.bytes('/api/files/download?device=admin/source-photos&path=sub/b.txt')
  check(gone.status === 200 && gone.body.toString() === 'in a sub folder', 'a file deleted at the source stays recoverable (retention)')

  // --- a storage that cannot be listed must not look like mass deletion ------------------------------------------------------------------
  fs.renameSync(photos, `${photos}.off`)
  const failed = await runAndWait('sources', 'photos')
  check(failed[0] === 'failed', `a failing listing fails the run (${failed[1].slice(0, 80)})`)
  check((await livePaths('admin/source-photos')).size === paths.size, 'nothing was deleted because the storage was unreachable')
  check((await openAlerts()).some((a) => a.kind === 'source_failed' && a.key === 'admin/source-photos'), 'a failed run raises an alert on the dashboard')
  fs.renameSync(`${photos}.off`, photos)
  const recovered = await runAndWait('sources', 'photos')
  check(recovered[0] === 'ok' && !(await openAlerts()).some((a) => a.kind === 'source_failed'), 'the alert clears after a good run')

  // --- a device too, so the mirror has both kinds -----------------------------------------------------------------------------------
  const { c } = await enrollDevice(h, admin)
  const docs = path.join(tmp, 'docs')
  fs.mkdirSync(path.join(docs, 'deep'), { recursive: true })
  fs.writeFileSync(path.join(docs, 'x.txt'), 'from a device')
  fs.writeFileSync(path.join(docs, 'deep', 'y.txt'), 'deeper')
  c('add-folder', docs, '--name', 'docs')
  c('sync')

  // --- mirror ----------------------------------------------------------------------------------------------------------------------------------
  const mirrorDir = path.join(tmp, 'offsite')
  const form = { name: 'offsite', remote_type: 'local', options: '', remote_path: mirrorDir.replaceAll('\\', '/'), interval_hours: 72, keep_runs: 3 }
  check((await admin.post('/api/mirrors', form)).status === 200, 'a mirror is added through the web API')
  const m1 = await until(() => finished('mirrors', 'offsite'), 90)
  check(m1?.[0] === 'ok', `the first mirror run succeeds (${m1?.[0]}: ${JSON.stringify(m1?.[1])})`)
  let files = tree(path.join(mirrorDir, 'files'))
  check(files['users/admin/source-photos/a.txt']?.toString() === 'second version' && files['users/admin/source-photos/big-renamed.bin']?.equals(big), 'the mirror holds the current files of the source as a plain tree')
  check(files['users/admin/laptop/docs/x.txt']?.toString() === 'from a device' && files['users/admin/laptop/docs/deep/y.txt']?.toString() === 'deeper', 'the mirror holds device files under files/users/<user>/<name>/, the same layout as the master')
  check(!('users/admin/source-photos/sub/b.txt' in files) && !('users/admin/source-photos/big.bin' in files), 'deleted and renamed-away files are not in the current tree')
  const stats = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'stats.json'), 'utf8'))
  check(stats.files === Object.keys(files).length && new Set(stats.namespaces.map((n) => n.namespace)).size === 2, `stats.json describes the tree (${stats.files} files)`)
  const listing = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'admin_laptop.json'), 'utf8'))
  check(listing.files.length === 2 && listing.files.every((f) => f.root_hash && f.size), 'a file list per device is written as JSON')
  const library = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'library.json'), 'utf8'))
  check(Array.isArray(library.albums) && Array.isArray(library.tags) && Array.isArray(library.files), 'albums, tags and what was said about files are written as JSON too')
  check(
    Object.entries(files).filter(([k]) => k.startsWith('users/admin/laptop/')).every(([k, v]) => v.length === listing.files.find((f) => `users/admin/laptop/${f.path}` === k).size),
    'file list sizes match the mirrored files',
  )
  check(Number((await schema.rows('SELECT last_files FROM mirrors'))[0][0]) === Object.keys(files).length, 'the mirrored file count is recorded')

  // --- a mirror of one folder holds only that folder, at the same place below files/ ---------------------------------------------
  const scopedDir = path.join(tmp, 'offsite-docs')
  const scoped = { ...form, name: 'docs-only', remote_path: scopedDir.replaceAll('\\', '/'), scope: 'admin/laptop/docs' }
  check((await admin.post('/api/mirrors', { ...scoped, name: 'nowhere', scope: 'admin/nope' })).status === 400, 'a folder that does not exist cannot be mirrored')
  check((await admin.get('/api/mirrors/scope?path=admin%2Flaptop')).json.some((c) => c.name === 'docs' && c.kind === 'folder'), 'the folders of a device are offered to pick from')
  check((await admin.post('/api/mirrors', scoped)).status === 200, 'a mirror of one folder is added')
  const m3 = await until(() => finished('mirrors', 'docs-only'), 90)
  check(m3?.[0] === 'ok', `the mirror of one folder succeeds (${m3?.[0]}: ${JSON.stringify(m3?.[1])})`)
  check(
    Object.keys(tree(path.join(scopedDir, 'files'))).sort().join() === 'users/admin/laptop/docs/deep/y.txt,users/admin/laptop/docs/x.txt',
    'it holds only the files of that folder, with the same paths as the master',
  )
  check(fs.existsSync(path.join(scopedDir, 'meta', 'stats.json')) && !fs.existsSync(path.join(scopedDir, 'meta', 'library.json')), 'it leaves out the library file, which names files of the whole tree')

  // --- an incremental run uploads only what changed; replaced and removed files go to versions/ ---------------------------------
  fs.writeFileSync(path.join(docs, 'x.txt'), 'device edit')
  c('sync')
  put('a.txt', 'third version')
  fs.rmSync(path.join(photos, 'new.txt'))
  await runAndWait('sources', 'photos')
  const m2 = await runAndWait('mirrors', 'offsite')
  check(m2[0] === 'ok' && m2[1].startsWith('2 uploaded'), `only the two changed files are uploaded (${m2[1].split('\n')[0]})`)
  files = tree(path.join(mirrorDir, 'files'))
  check(files['users/admin/source-photos/a.txt']?.toString() === 'third version' && files['users/admin/laptop/docs/x.txt']?.toString() === 'device edit', 'the changes reached the mirror')
  check(!('users/admin/source-photos/new.txt' in files), 'a file deleted at the source is gone from the current tree')
  const kept = Object.fromEntries(Object.entries(tree(path.join(mirrorDir, 'versions'))).map(([k, v]) => [k.split('/').slice(1).join('/'), v]))
  check(kept['users/admin/source-photos/a.txt']?.toString() === 'second version' && kept['users/admin/laptop/docs/x.txt']?.toString() === 'from a device', 'replaced files are kept under versions/<run>/')
  check(kept['users/admin/source-photos/new.txt']?.toString() === 'fresh', 'removed files are kept under versions/<run>/')

  // --- only the newest runs are kept ------------------------------------------------------------------------------------------------------
  for (let n = 0; n < 5; n++) {
    put('a.txt', `revision ${n}`)
    await runAndWait('sources', 'photos')
    await runAndWait('mirrors', 'offsite')
  }
  const dirs = fs.readdirSync(path.join(mirrorDir, 'versions')).sort()
  check(dirs.length === 3, `old version runs are pruned to the newest 3 (${dirs})`)
  check(new Set(dirs).size === 3 && dirs.every((d) => /^\d{8}T\d{6}Z$/.test(d)), 'version directories are UTC timestamps')

  // --- a rename is a move on the mirror, not an upload --------------------------------------------------------------------------------------
  const vaultDir = path.join(tmp, 'vault')
  const passwords = { crypt_password: 'a secret password!', crypt_salt: 'and another salt' }
  const vault = { ...form, name: 'vault', kind: 'backup', remote_path: vaultDir.replaceAll('\\', '/'), ...passwords }
  check((await admin.post('/api/mirrors', { ...vault, name: 'weak', crypt_password: 'short' })).status === 400, 'a backup needs a long enough key')
  check((await admin.post('/api/mirrors', { ...vault, name: 'nokey', crypt_salt: '' })).status === 400, 'a backup needs both passwords')
  fs.writeFileSync(path.join(docs, 'move-me.txt'), 'a file that will move '.repeat(20000))
  c('sync')
  await runAndWait('mirrors', 'offsite')
  fs.mkdirSync(path.join(docs, 'moved'))
  fs.renameSync(path.join(docs, 'move-me.txt'), path.join(docs, 'moved', 'moved.txt'))
  c('sync')
  const moved = await runAndWait('mirrors', 'offsite')
  check(moved[0] === 'ok' && moved[1].startsWith('0 uploaded') && moved[1].includes('1 moved'), `a renamed file is moved on the mirror, not uploaded again (${moved[1].split('\n')[0]})`)
  files = tree(path.join(mirrorDir, 'files'))
  check(files['users/admin/laptop/docs/moved/moved.txt']?.toString().startsWith('a file that will move') && !('users/admin/laptop/docs/move-me.txt' in files), 'the file is at its new path and gone from the old one')

  // --- a backup is compressed and encrypted; its restore kit brings the files back with plain rclone --------------------------------
  check((await admin.post('/api/mirrors', vault)).status === 200, 'a backup mirror is added')
  const v1 = await until(() => finished('mirrors', 'vault'), 90)
  check(v1?.[0] === 'ok', `the backup runs (${v1?.[0]}: ${JSON.stringify(v1?.[1])})`)
  const raw = tree(vaultDir)
  const rawNames = Object.keys(raw).join('\n')
  check(!/x\.txt|move-me|Photos|laptop|files|meta/.test(rawNames), 'names of files and folders are encrypted')
  check(![...Object.values(raw)].some((b) => b.includes('from a device') || b.includes('device edit') || b.includes('a file that will move')), 'contents are encrypted')
  const kit = (await admin.get('/api/mirrors/vault/restore-kit')).json.config
  check(kit.includes('type = crypt') && kit.includes('type = compress') && !kit.includes(passwords.crypt_password), 'the restore kit is an rclone config with the key obscured')
  check((await admin.get('/api/mirrors/offsite/restore-kit')).status === 400, 'a replica has no restore kit')
  check(!JSON.stringify((await admin.get('/api/mirrors')).json).includes('secret password'), 'the key is not in the mirror list')
  const kitFile = path.join(tmp, 'restore.conf')
  fs.writeFileSync(kitFile, kit)
  const restored = path.join(tmp, 'restored')
  execFileSync(rclone, ['--config', kitFile, 'copy', 'fafnirx-backup:files', restored])
  const back = tree(restored)
  files = tree(path.join(mirrorDir, 'files'))
  check(Object.keys(back).sort().join() === Object.keys(files).sort().join() && Object.entries(back).every(([k, v]) => v.equals(files[k])), `a restore through the kit gives back every file, byte for byte (${Object.keys(back).length} files)`)
  const compressed = [...Object.values(raw)].reduce((n, b) => n + b.length, 0)
  const original = Object.values(files).reduce((n, b) => n + b.length, 0)
  check(compressed < original - 400_000, `the backup is compressed: the 440 KB repetitive file is stored in a fraction of that (${compressed} stored for ${original} bytes)`)
  fs.renameSync(path.join(docs, 'moved', 'moved.txt'), path.join(docs, 'moved', 'again.txt'))
  c('sync')
  const vmoved = await runAndWait('mirrors', 'vault')
  check(vmoved[0] === 'ok' && vmoved[1].includes('1 moved'), `a rename also moves inside an encrypted backup (${vmoved[1].split('\n')[0]})`)
  execFileSync(rclone, ['--config', kitFile, 'copy', 'fafnirx-backup:files', path.join(tmp, 'restored2')])
  check('users/admin/laptop/docs/moved/again.txt' in tree(path.join(tmp, 'restored2')) && !('users/admin/laptop/docs/moved/moved.txt' in tree(path.join(tmp, 'restored2'))), 'the restore has the new name only')
  await runAndWait('mirrors', 'offsite')

  // --- bit rot in the stored files is caught before it reaches the mirror ---------------------------------------------------------------------
  const rotten = path.join(data, 'files', 'users', 'admin', 'laptop', 'docs', 'deep', 'y.txt')
  const blob = fs.readFileSync(rotten)
  blob[0] ^= 1
  fs.writeFileSync(rotten, blob)
  await admin.post('/api/mirrors/offsite/resync')
  const partial = await until(async () => {
    const s = await status('mirrors', 'offsite')
    return s && s[0] !== 'running' && s[2] && s[1]?.includes('corrupt') ? s : null
  }, 90)
  check(partial?.[0] === 'partial' && partial[1].includes('docs/deep/y.txt'), `a corrupt stored file is reported, the rest still mirrors (${partial?.[0]})`)
  check((await openAlerts()).some((a) => a.kind === 'mirror_failed'), 'a partial mirror run raises an alert')
  check(tree(path.join(mirrorDir, 'files'))['users/admin/laptop/docs/deep/y.txt']?.toString() === 'deeper', 'the previously good copy on the mirror was not overwritten with corrupt data')

  // --- a broken target fails loudly ---------------------------------------------------------------------------------------------------------------
  await admin.post('/api/mirrors', { ...form, name: 'broken', remote_type: 'doesnotexist' })
  const broken = await until(() => finished('mirrors', 'broken'), 90)
  check(broken?.[0] === 'failed', `an unusable mirror fails with a message (${broken?.[1]?.slice(0, 90)})`)

  // --- a source of any rclone storage (here a plain directory) ------------------------------------------------------------------------------
  const shelf = path.join(tmp, 'shelf')
  fs.mkdirSync(path.join(shelf, 'sub'), { recursive: true })
  fs.writeFileSync(path.join(shelf, 'one.txt'), 'first')
  fs.writeFileSync(path.join(shelf, 'sub', 'two.txt'), 'second')
  const shelfForm = { name: 'shelf', remote_type: 'local', remote_path: shelf.replaceAll('\\', '/'), interval_hours: 24 }
  check((await admin.post('/api/sources', { ...shelfForm, name: 'bad name!' })).status === 400, 'a source needs a valid name')
  check((await admin.post('/api/sources', { ...shelfForm, remote_type: '' })).status === 400, 'a source needs a backend or a remote')
  check((await admin.post('/api/sources', shelfForm)).status === 200, 'an rclone source is added through the web API')
  const r1 = await until(() => finished('sources', 'shelf'), 90)
  check(r1?.[0] === 'ok' && r1[1].startsWith('2 added'), `the first run copies both files (${r1?.[0]}: ${r1?.[1]?.split('\n')[0]})`)
  const shelved = tree(path.join(data, 'files', 'users', 'admin', 'source-shelf'))
  check(shelved['one.txt']?.toString() === 'first' && shelved['sub/two.txt']?.toString() === 'second', 'they are under files/users/<user>/source-<name>/ with the remote layout')
  const listed = (await admin.get('/api/sources')).json.find((s) => s.name === 'shelf')
  check(listed?.detail.kind === 'rclone' && String(listed.detail.remote).startsWith('local:'), 'it is listed with its kind and storage')
  fs.writeFileSync(path.join(shelf, 'one.txt'), 'first, edited')
  fs.rmSync(path.join(shelf, 'sub', 'two.txt'))
  fs.writeFileSync(path.join(shelf, 'three.txt'), 'third')
  const r2 = await runAndWait('sources', 'shelf')
  check(r2[0] === 'ok' && r2[1].startsWith('1 added, 1 updated') && r2[1].includes('1 deleted'), `edits, additions and deletions are picked up (${r2[1].split('\n')[0]})`)
  check(tree(path.join(data, 'files', 'users', 'admin', 'source-shelf'))['one.txt']?.toString() === 'first, edited', 'the edited file has its new content')
  fs.rmSync(shelf, { recursive: true })
  const r3 = await runAndWait('sources', 'shelf')
  check(r3[0] === 'failed', `a storage that cannot be listed fails the run and deletes nothing (${r3[0]})`)

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  await h.cleanup()
}
