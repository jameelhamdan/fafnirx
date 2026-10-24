// End-to-end test for sources (Google Drive) and mirrors (rclone).
//
// A fake Google Drive HTTP server stands in for Google: it verifies the RS256 service-account assertion,
// pages its listings, and can fail on demand. The mirror target is a local directory through rclone's `local`
// backend. Everything else (the master, the client) is the real thing. Needs rclone on PATH or in $FAFNIRX_RCLONE.
//   cd tests && npm install && node e2e-io.mjs
import { createHash, createVerify, generateKeyPairSync, randomBytes } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { CHUNK, check, enrollDevice, freePort, harness, passedChecks, sleep, until } from './lib.mjs'

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

const FOLDER = 'ROOTFOLDER0001'
const SUBFOLDER = 'SUBFOLDER0002'
const DOC = 'application/vnd.google-apps.document'
const FOLDER_MIME = 'application/vnd.google-apps.folder'
const b64url = (b) => Buffer.from(b, 'base64url')

/** Just enough of the Drive v3 API: token endpoint, files.get, files.list, alt=media. */
class FakeDrive {
  items = new Map()
  failListing = false
  flakyOnce = new Set()
  mediaRequests = []
  tokensIssued = 0

  constructor(publicKey, port) {
    this.port = port
    this.server = http.createServer((req, res) => {
      const send = (code, body, type = 'application/json') => {
        const raw = Buffer.isBuffer(body) ? body : Buffer.from(JSON.stringify(body))
        res.writeHead(code, { 'content-type': type, 'content-length': raw.length })
        res.end(raw)
      }
      const url = new URL(req.url, 'http://x')
      if (req.method === 'POST') {
        let body = ''
        req.on('data', (d) => (body += d))
        req.on('end', () => {
          try {
            const form = new URLSearchParams(body)
            if (form.get('grant_type') !== 'urn:ietf:params:oauth:grant-type:jwt-bearer') throw new Error('grant type')
            const [h, p, s] = form.get('assertion').split('.')
            if (!createVerify('RSA-SHA256').update(`${h}.${p}`).verify(publicKey, b64url(s))) throw new Error('bad signature')
            const claims = JSON.parse(b64url(p).toString())
            if (claims.aud !== `http://127.0.0.1:${port}/token` || !claims.scope.endsWith('drive.readonly') || !claims.iss.startsWith('bot@')) throw new Error('claims')
            this.tokensIssued++
            send(200, { access_token: 'test-token', expires_in: 3600 })
          } catch (e) {
            send(400, { error: 'invalid_grant', error_description: String(e) })
          }
        })
        return
      }
      if (req.headers.authorization !== 'Bearer test-token') return send(401, { error: 'unauthorized' })
      if (url.pathname === '/drive/v3/files') {
        if (this.failListing) return send(403, { error: { message: 'forbidden' } })
        const parent = /^'([^']+)' in parents and trashed = false/.exec(url.searchParams.get('q'))[1]
        const kids = [...this.items].filter(([, v]) => v.parent === parent).map(([id]) => id).sort()
        const start = Number(url.searchParams.get('pageToken') ?? 0)
        const out = { files: kids.slice(start, start + 2).map((id) => this.describe(id)) } // tiny pages exercise pagination
        if (start + 2 < kids.length) out.nextPageToken = String(start + 2)
        return send(200, out)
      }
      const id = url.pathname.split('/').pop()
      const item = this.items.get(id)
      if (!item) return send(404, { error: { message: 'not found' } })
      if (url.searchParams.get('alt') === 'media') {
        this.mediaRequests.push(id)
        if (this.flakyOnce.delete(id)) return send(503, { error: 'try later' })
        return send(200, item.data, 'application/octet-stream')
      }
      send(200, { name: item.name })
    })
  }

  listen() {
    return new Promise((r) => this.server.listen(this.port, '127.0.0.1', r))
  }

  describe(id) {
    const v = this.items.get(id)
    const d = { id, name: v.name, mimeType: v.mime, modifiedTime: v.modified }
    if (v.data !== null) {
      d.size = String(v.data.length)
      d.md5Checksum = createHash('md5').update(v.data).digest('hex')
    }
    return d
  }

  put(id, name, data = Buffer.alloc(0), { parent = FOLDER, mime = 'application/octet-stream', modified = '2024-05-01T10:00:00.000Z' } = {}) {
    this.items.set(id, { name, mime, parent, data: data === null ? null : Buffer.from(data), modified })
  }
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

const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 })
const pem = privateKey.export({ type: 'pkcs8', format: 'pem' })
const drive = new FakeDrive(publicKey, await freePort())
await drive.listen()
const sa = { client_email: 'bot@proj.iam.gserviceaccount.com', private_key: pem, token_uri: `http://127.0.0.1:${drive.port}/token` }
const h = await harness('fafnirx-e2e-io-', {
  env: { FAFNIRX_GDRIVE_API: `http://127.0.0.1:${drive.port}`, FAFNIRX_RCLONE: rclone },
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
const stored = (device, p) => fs.readFileSync(path.join(data, 'files', device, p))
const openAlerts = async () => (await admin.get('/api/dashboard')).json.alerts

try {
  await master.start()
  admin = await master.admin()

  // --- Google Drive source -----------------------------------------------------------------------------------------------
  const big = randomBytes(CHUNK * 2 + 777)
  drive.put(FOLDER, 'Photos', null, { parent: 'root', mime: FOLDER_MIME })
  drive.put('FILE000001', 'a.txt', 'first version')
  drive.put('FILE000002', 'big.bin', big)
  drive.put('FILE000003', 'dup.txt', 'one')
  drive.put('FILE000004', 'dup.txt', 'two')
  drive.put('FILE000005', 'Notes', '', { mime: DOC })
  drive.put('FILE000006', 'shortcut', null, { mime: 'application/vnd.google-apps.shortcut' })
  drive.put('FILE000007', 'con/tains:bad?.txt', 'odd name')
  drive.put(SUBFOLDER, 'sub', null, { mime: FOLDER_MIME })
  drive.put('FILE000008', 'b.txt', 'in a sub folder', { parent: SUBFOLDER })
  drive.flakyOnce.add('FILE000001')

  const added = await admin.post('/api/sources', { name: 'drive', folders: FOLDER, service_account: JSON.stringify(sa), interval_hours: 24 })
  check(added.status === 200, `a google drive source is added through the web API (${added.json?.error ?? ''})`)
  const first = await until(() => finished('sources', 'drive'), 90)
  check(first?.[0] === 'ok', `the first run succeeds (${first?.[0]}: ${JSON.stringify(first?.[1])})`)
  check(drive.tokensIssued >= 1, 'the service account assertion was signed and accepted (RS256)')

  let paths = await livePaths('admin/source-drive')
  const want = ['Photos/a.txt', 'Photos/big.bin', 'Photos/dup.txt', 'Photos/dup (FILE00).txt', 'Photos/con_tains_bad_.txt', 'Photos/sub/b.txt']
  check(paths.size === want.length && want.every((p) => paths.has(p)), `the source holds raw files only, names made safe and unique (${[...paths].sort()})`)
  check(first[1].includes('2 skipped'), 'native docs and shortcuts are skipped and counted')
  check(stored('admin/source-drive', 'Photos/big.bin').equals(big), 'a multi-chunk Drive file is stored byte for byte under files/<source>/')
  check(stored('admin/source-drive', 'Photos/a.txt').toString() === 'first version', 'a transient 503 from Drive was retried')
  const devices = (await admin.get('/api/devices')).json
  check(devices.filter((d) => d.kind === 'device').length === 0 && devices.some((d) => d.kind === 'source'), 'a source is listed as a source, never as a device')

  // --- changes in Drive: edit, delete, rename, add --------------------------------------------------------------------------
  const downloadsBefore = drive.mediaRequests.filter((i) => i === 'FILE000002').length
  drive.put('FILE000001', 'a.txt', 'second version', { modified: '2024-06-01T10:00:00.000Z' })
  drive.items.delete('FILE000008')
  drive.items.get('FILE000002').name = 'big-renamed.bin'
  drive.put('FILE000009', 'new.txt', 'fresh')
  const second = await runAndWait('sources', 'drive')
  check(second[0] === 'ok', `the second run succeeds (${JSON.stringify(second[1])})`)
  paths = await livePaths('admin/source-drive')
  check(paths.has('Photos/big-renamed.bin') && !paths.has('Photos/big.bin') && paths.has('Photos/new.txt') && !paths.has('Photos/sub/b.txt'), 'edit, rename, add and delete in Drive are all picked up')
  check(drive.mediaRequests.filter((i) => i === 'FILE000002').length === downloadsBefore, 'a renamed file is not downloaded again')
  check(stored('admin/source-drive', 'Photos/a.txt').toString() === 'second version', 'an edited file has the new content')
  const info = (await admin.get('/api/files/info?device=admin/source-drive&path=Photos/a.txt')).json
  check(info.versions.length === 2, 'the previous version is kept')
  const gone = await admin.bytes('/api/files/download?device=admin/source-drive&path=Photos/sub/b.txt')
  check(gone.status === 200 && gone.body.toString() === 'in a sub folder', 'a file deleted in Drive stays recoverable (retention)')
  const [ev] = await schema.rows("SELECT action, detail FROM events WHERE path = 'Photos/big-renamed.bin'")
  check(ev?.[0] === 'rename' && ev?.[1] === 'Photos/big.bin', 'the rename is recorded in the change log')

  // --- Drive failure must not look like mass deletion --------------------------------------------------------------------------
  drive.failListing = true
  const failed = await runAndWait('sources', 'drive')
  check(failed[0] === 'failed', `a failing listing fails the run (${failed[1].slice(0, 80)})`)
  check((await livePaths('admin/source-drive')).size === paths.size, 'nothing was deleted because Drive was unreachable')
  check((await openAlerts()).some((a) => a.kind === 'source_failed' && a.key === 'admin/source-drive'), 'a failed run raises an alert on the dashboard')
  drive.failListing = false
  const recovered = await runAndWait('sources', 'drive')
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
  check(files['admin/source-drive/Photos/a.txt']?.toString() === 'second version' && files['admin/source-drive/Photos/big-renamed.bin']?.equals(big), 'the mirror holds the current Drive files as a plain tree')
  check(files['admin/laptop/docs/x.txt']?.toString() === 'from a device' && files['admin/laptop/docs/deep/y.txt']?.toString() === 'deeper', 'the mirror holds device files under files/<name>/, the same layout as the master')
  check(!('admin/source-drive/Photos/sub/b.txt' in files) && !('admin/source-drive/Photos/big.bin' in files), 'deleted and renamed-away files are not in the current tree')
  const stats = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'stats.json'), 'utf8'))
  check(stats.files === Object.keys(files).length && new Set(stats.namespaces.map((n) => n.namespace)).size === 2, `stats.json describes the tree (${stats.files} files)`)
  const listing = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'admin_laptop.json'), 'utf8'))
  check(listing.files.length === 2 && listing.files.every((f) => f.root_hash && f.size), 'a file list per device is written as JSON')
  const library = JSON.parse(fs.readFileSync(path.join(mirrorDir, 'meta', 'library.json'), 'utf8'))
  check(Array.isArray(library.albums) && Array.isArray(library.tags) && Array.isArray(library.files), 'albums, tags and what was said about files are written as JSON too')
  check(
    Object.entries(files).filter(([k]) => k.startsWith('admin/laptop/')).every(([k, v]) => v.length === listing.files.find((f) => `admin/laptop/${f.path}` === k).size),
    'file list sizes match the mirrored files',
  )
  check(Number((await schema.rows('SELECT last_files FROM mirrors'))[0][0]) === Object.keys(files).length, 'the mirrored file count is recorded')

  // --- an incremental run uploads only what changed; replaced and removed files go to versions/ ---------------------------------
  fs.writeFileSync(path.join(docs, 'x.txt'), 'device edit')
  c('sync')
  drive.put('FILE000001', 'a.txt', 'third version', { modified: '2024-07-01T10:00:00.000Z' })
  drive.items.delete('FILE000009') // new.txt removed
  await runAndWait('sources', 'drive')
  const m2 = await runAndWait('mirrors', 'offsite')
  check(m2[0] === 'ok' && m2[1].startsWith('2 uploaded'), `only the two changed files are uploaded (${m2[1].split('\n')[0]})`)
  files = tree(path.join(mirrorDir, 'files'))
  check(files['admin/source-drive/Photos/a.txt']?.toString() === 'third version' && files['admin/laptop/docs/x.txt']?.toString() === 'device edit', 'the changes reached the mirror')
  check(!('admin/source-drive/Photos/new.txt' in files), 'a file deleted at the source is gone from the current tree')
  const kept = Object.fromEntries(Object.entries(tree(path.join(mirrorDir, 'versions'))).map(([k, v]) => [k.split('/').slice(1).join('/'), v]))
  check(kept['admin/source-drive/Photos/a.txt']?.toString() === 'second version' && kept['admin/laptop/docs/x.txt']?.toString() === 'from a device', 'replaced files are kept under versions/<run>/')
  check(kept['admin/source-drive/Photos/new.txt']?.toString() === 'fresh', 'removed files are kept under versions/<run>/')

  // --- only the newest runs are kept ------------------------------------------------------------------------------------------------------
  for (let n = 0; n < 5; n++) {
    drive.put('FILE000001', 'a.txt', `revision ${n}`, { modified: `2024-08-0${n + 1}T10:00:00.000Z` })
    await runAndWait('sources', 'drive')
    await runAndWait('mirrors', 'offsite')
  }
  const dirs = fs.readdirSync(path.join(mirrorDir, 'versions')).sort()
  check(dirs.length === 3, `old version runs are pruned to the newest 3 (${dirs})`)
  check(new Set(dirs).size === 3 && dirs.every((d) => /^\d{8}T\d{6}Z$/.test(d)), 'version directories are UTC timestamps')

  // --- bit rot in the stored files is caught before it reaches the mirror ---------------------------------------------------------------------
  const rotten = path.join(data, 'files', 'admin', 'laptop', 'docs', 'deep', 'y.txt')
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
  check(tree(path.join(mirrorDir, 'files'))['admin/laptop/docs/deep/y.txt']?.toString() === 'deeper', 'the previously good copy on the mirror was not overwritten with corrupt data')

  // --- a broken target fails loudly ---------------------------------------------------------------------------------------------------------------
  await admin.post('/api/mirrors', { ...form, name: 'broken', remote_type: 'doesnotexist' })
  const broken = await until(() => finished('mirrors', 'broken'), 90)
  check(broken?.[0] === 'failed', `an unusable mirror fails with a message (${broken?.[1]?.slice(0, 90)})`)

  // --- credentials never leak back ---------------------------------------------------------------------------------------------------------------------
  const pages = JSON.stringify((await admin.get('/api/sources')).json) + JSON.stringify((await admin.get('/api/mirrors')).json)
  check(!pages.includes('PRIVATE KEY') && !pages.includes(pem.slice(40, 80)), 'the service account key is never sent back to the browser')
  const logs = fs.readFileSync(path.join(tmp, 'master.log'), 'utf8')
  check(!logs.includes('PRIVATE KEY') && !logs.includes('test-token'), 'credentials and tokens are not logged')

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  drive.server.close()
  await h.cleanup()
}
