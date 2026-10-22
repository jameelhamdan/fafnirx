// End-to-end test of the split roles: an api, a transfer and a worker process share one database and one data
// directory, and cooperate only through PostgreSQL (advisory locks, LISTEN/NOTIFY, the job queue).
//   cd tests && npm install && node e2e-roles.mjs
import fs from 'node:fs'
import path from 'node:path'
import { CLIENT, MASTER, Master, check, clientIn, createSchema, freePort, mkdtemp, passedChecks, until } from './lib.mjs'

if (!fs.existsSync(MASTER) || !fs.existsSync(CLIENT)) {
  console.error('missing binaries; run cargo build --workspace first')
  process.exit(1)
}

// A 3 x 2 pixel PNG.
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAMAAAACCAIAAAASFvFNAAAAEElEQVR4nGM4wcUFQQxwFgA4GAUpBor2lAAAAABJRU5ErkJggg==', 'base64')

const tmp = mkdtemp('fafnirx-e2e-roles-')
console.log('working in', tmp)
const data = path.join(tmp, 'data')
fs.mkdirSync(data)
const schema = await createSchema()
const common = { data, dbUrl: schema.url }
const api = new Master({ ...common, http: await freePort(), grpc: await freePort(), env: { FAFNIRX_ROLE: 'api' } })
const transfer = new Master({ ...common, http: await freePort(), grpc: await freePort(), env: { FAFNIRX_ROLE: 'transfer' } })
const worker = new Master({ ...common, http: await freePort(), grpc: await freePort(), env: { FAFNIRX_ROLE: 'worker' } })
const all = [api, transfer, worker]

try {
  for (const m of all) await m.start()
  const admin = await api.admin()
  // The login cookie is signed with a key in the data directory, so every process accepts it.
  const cookie = (await fetch(`${api.base}/api/login`, { method: 'POST', headers: { 'content-type': 'application/json', 'x-requested-with': 'e2e' }, body: JSON.stringify({ password: 'pw' }) })).headers.get('set-cookie').split(';')[0]

  const made = (await admin.post('/api/devices', { name: 'laptop' })).json
  const { cfg, c } = clientIn(path.join(tmp, 'client'))
  c.enroll(made.token, `http://127.0.0.1:${transfer.grpc}`)
  fs.writeFileSync(cfg, `state_dir = "${path.join(tmp, 'state').replaceAll('\\', '/')}"\n${fs.readFileSync(cfg, 'utf8')}`)
  const src = path.join(tmp, 'docs')
  fs.mkdirSync(src)
  c('add-folder', src, '--name', 'docs')

  // --- live events cross the process boundary: the transfer process writes, the api process tells the browser ---
  const seen = []
  const abort = new AbortController()
  const stream = await fetch(`${api.base}/api/events`, { headers: { cookie }, signal: abort.signal })
  void (async () => {
    const decoder = new TextDecoder()
    try {
      for await (const part of stream.body) seen.push(decoder.decode(part))
    } catch {
      /* closed */
    }
  })()
  fs.writeFileSync(path.join(src, 'a.txt'), 'hello from the device')
  check(c('sync').stdout.includes('1 uploaded'), 'a device uploads through the transfer process')
  check(await until(() => seen.join('').includes('"topic":"files"'), 10), 'the api process pushes the change to the browser (LISTEN/NOTIFY across processes)')
  abort.abort()

  // --- each process serves only its routes --------------------------------------------------------------------------
  check((await admin.get('/api/files?device=laptop&path=docs')).json.entries.some((e) => e.name === 'a.txt'), 'the api process lists what the transfer process stored')
  check((await admin.bytes('/api/files/download?device=laptop&path=docs/a.txt')).status === 404, 'downloads are not served by the api process')
  const dl = await fetch(`${transfer.base}/api/files/download?device=laptop&path=docs/a.txt`, { headers: { cookie } })
  check(dl.status === 200 && (await dl.text()) === 'hello from the device', 'downloads are served by the transfer process, with the same login')
  check((await fetch(`${transfer.base}/api/dashboard`, { headers: { cookie } })).status === 404, 'the dashboard is not served by the transfer process')

  // --- the worker reads photo metadata from the shared job queue ----------------------------------------------------------
  await admin.post('/api/devices', { name: 'photos', kind: 'local' })
  const up = await (await fetch(`${transfer.base}/api/uploads`, { method: 'POST', headers: { cookie, 'x-requested-with': 'e2e', 'content-type': 'application/json' }, body: JSON.stringify({ device: 'photos', path: 'tiny.png', size: PNG.length }) })).json()
  await fetch(`${transfer.base}/api/uploads/${up.id}?offset=0`, { method: 'PUT', headers: { cookie, 'x-requested-with': 'e2e' }, body: PNG })
  await fetch(`${transfer.base}/api/uploads/${up.id}/finish`, { method: 'POST', headers: { cookie, 'x-requested-with': 'e2e', 'content-type': 'application/json' }, body: '{}' })
  const sized = await until(async () => (await admin.get('/api/files/info?device=photos&path=tiny.png')).json?.width === 3, 20)
  check(sized, 'the worker process read the image size from the job queue')
  const info = (await admin.get('/api/files/info?device=photos&path=tiny.png')).json
  check(info.height === 2 && info.mime === 'image/png', 'with its height and type')

  // --- the photo library across the processes: the api lists, the worker makes pictures, transfer serves them ----------------
  const asset = (await admin.get(`/api/timeline?month=${(await admin.get('/api/timeline/buckets')).json.months[0].month}`)).json.assets.find((a) => a.name === 'tiny.png')
  check(asset && asset.kind === 'image', 'the api process lists a photo in the timeline')
  const thumb = await fetch(`${transfer.base}/api/media/${asset.id}/thumb?v=${asset.hash}`, { headers: { cookie } })
  check(thumb.status === 200 && thumb.headers.get('content-type') === 'image/jpeg' && /immutable/.test(thumb.headers.get('cache-control')), 'the transfer process serves its picture, cacheable for good')
  check((await admin.bytes(`/api/media/${asset.id}/thumb`)).status === 404, 'pictures are not served by the api process')
  const pictures = () => fs.readdirSync(path.join(data, 'cache', 'thumbs')).filter((f) => f.endsWith('.jpg'))
  check(await until(() => pictures().length === 2, 10), 'the worker made both sizes of the picture (320 and 1600 px)')

  // --- only the worker watches the file tree --------------------------------------------------------------------------------
  fs.mkdirSync(path.join(data, 'files', 'photos', 'dropped'), { recursive: true })
  fs.writeFileSync(path.join(data, 'files', 'photos', 'dropped', 'x.txt'), 'copied into the share')
  check(await until(async () => (await admin.get('/api/files?device=photos&path=dropped')).json.entries.length === 1, 60), 'the worker process records a file dropped into files/')

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  for (const m of all) await m.stop()
  await schema.drop()
  if (process.env.FAFNIRX_KEEP === '1') console.log('kept', tmp)
  else fs.rmSync(tmp, { recursive: true, force: true })
}
