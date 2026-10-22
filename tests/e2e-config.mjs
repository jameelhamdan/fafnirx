// End-to-end test of the configuration file (fafnirx.json) and of mirrors that name an rclone remote.
//
// The real master is started from a file, changed through the admin API (which writes the file) and through the file
// (which applies at the next start), refused when the file is wrong, and then started again on an empty database from
// the same file: the devices keep their tokens, the mirrors come back and run. The mirror
// target is a directory reached through a remote of an rclone.conf. Needs rclone on PATH or in $FAFNIRX_RCLONE.
//   cd tests && npm install && node e2e-config.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { check, clientIn, createSchema, ENV, freePort, harness, MASTER, passedChecks, until } from './lib.mjs'

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

const h = await harness('fafnirx-e2e-cfg-')
const { tmp, master } = h
const forward = (p) => p.replaceAll('\\', '/')
const configDir = path.join(tmp, 'config')
const configFile = path.join(configDir, 'fafnirx.json')
const mirrorDir = path.join(tmp, 'mirror-vault')
const secondDir = path.join(tmp, 'mirror-second')
fs.mkdirSync(configDir)
fs.mkdirSync(mirrorDir)
fs.mkdirSync(secondDir)
// A remote of the rclone config, as a person would have it (here a plain directory).
fs.writeFileSync(path.join(configDir, 'rclone.conf'), `[dest]\ntype = local\n\n[second]\ntype = local\n`)
master.env = {
  FAFNIRX_CONFIG: configFile,
  FAFNIRX_RCLONE: rclone,
  FAFNIRX_CONFIG_POLL_SECS: '1',
}

/** Runs the master binary once and returns how it ended; for a start that is expected to fail. */
async function startOnce(dbUrl) {
  const out = spawnSync(MASTER, [], {
    encoding: 'utf8',
    timeout: 20_000,
    env: {
      ...ENV,
      FAFNIRX_DATABASE_URL: dbUrl,
      FAFNIRX_DATA: master.data,
      FAFNIRX_LISTEN: `127.0.0.1:${await freePort()}`,
      FAFNIRX_HTTP: `127.0.0.1:${await freePort()}`,
      FAFNIRX_ADMIN_PASSWORD: 'pw',
      ...master.env,
    },
  })
  return { status: out.status, out: out.stderr + out.stdout }
}

const readFile = () => JSON.parse(fs.readFileSync(configFile, 'utf8'))
const writeFile = (c) => fs.writeFileSync(configFile, JSON.stringify(c, null, 2))
const names = (list) => (list ?? []).map((x) => x.name)
/** Waits until the file satisfies `fn`. */
const fileSays = (fn, seconds = 20) => until(() => { try { return fn(readFile()) } catch { return false } }, seconds, 200)

try {
  // --- a master that starts from a file written by hand -------------------------------------------------------
  writeFile({
    version: 1,
    settings: { retention_days: 12 },
    devices: [{ name: 'laptop', token: 'laptop-token-from-the-file' }, { name: 'photos', kind: 'local' }],
    mirrors: [{ name: 'vault', remote: `dest:${forward(mirrorDir)}`, interval_hours: 1, keep_runs: 2 }],
  })
  await master.start()
  let admin = await master.admin()
  check((await admin.get('/api/settings')).json.retention_days === 12, 'a setting in the file is applied at start')
  check(names((await admin.get('/api/devices')).json).join() === 'laptop,photos', 'the devices of the file exist')
  const vault = (await admin.get('/api/mirrors')).json.find((m) => m.name === 'vault')
  check(vault && vault.detail.remote.startsWith('dest:'), 'the mirror of the file exists and names its rclone remote')
  check(!JSON.stringify(vault).includes('token'), 'the web app shows no credentials')
  const settled = readFile()
  check(settled.settings.version_count === 5 && settled.settings.offline_after_secs === 60, 'the file is completed with the settings it left out')
  const hash = settled.devices.find((d) => d.name === 'laptop').token_hash
  check(/^[0-9a-f]{64}$/.test(hash) && !fs.readFileSync(configFile, 'utf8').includes('laptop-token-from-the-file'), 'a token written by hand is replaced by its hash')

  // --- the token from the file is the device's token; its files reach the mirror ---------------------------------
  const client = clientIn(path.join(tmp, 'client'))
  client.c.enroll('laptop-token-from-the-file', `http://127.0.0.1:${master.grpc}`)
  fs.writeFileSync(client.cfg, `state_dir = "${forward(path.join(tmp, 'state'))}"\n${fs.readFileSync(client.cfg, 'utf8')}`)
  const docs = path.join(tmp, 'docs')
  fs.mkdirSync(docs)
  fs.writeFileSync(path.join(docs, 'a.txt'), 'alpha')
  client.c('add-folder', docs)
  client.c('sync')
  check((await admin.get('/api/files?device=laptop')).status === 200 && fs.existsSync(path.join(master.data, 'files', 'laptop', 'docs', 'a.txt')), 'the device enrolled with that token sends its files')
  const ran = await until(() => fs.existsSync(path.join(mirrorDir, 'files', 'laptop', 'docs', 'a.txt')), 40, 300)
  check(ran, 'the mirror that names an rclone remote uploads the files')
  check(fs.existsSync(path.join(mirrorDir, 'meta', 'stats.json')), 'and its statistics')

  // --- the web app writes the file ---------------------------------------------------------------------------------
  check((await admin.post('/api/mirrors', { name: 'direct', remote_type: 'local', remote_path: forward(path.join(tmp, 'mirror-direct')), interval_hours: 24 })).status === 200, 'a mirror with a backend and options is added through the API')
  check((await admin.post('/api/mirrors', { name: 'nothing' })).status === 400, 'a mirror needs a backend or a remote')
  check(await fileSays((c) => names(c.mirrors).includes('direct')), 'a mirror added in the web app is written to the file')
  await admin.post('/api/mirrors/direct/toggle')
  check(await fileSays((c) => c.mirrors.find((m) => m.name === 'direct')?.enabled === false), 'pausing it in the web app is written to the file')
  await admin.put('/api/settings', { version_count: 8, retention_days: 20 })
  check(await fileSays((c) => c.settings.version_count === 8 && c.settings.retention_days === 20), 'saved settings are written to the file')
  const created = (await admin.post('/api/devices', { name: 'tablet' })).json
  check(await fileSays((c) => c.devices.find((d) => d.name === 'tablet')?.token_hash?.length === 64), 'a device made in the web app is in the file, as a hash')
  check(!fs.readFileSync(configFile, 'utf8').includes(created.token), 'the token itself is not')
  fs.mkdirSync(path.join(master.data, 'files', 'dropped'))
  check(await fileSays((c) => c.devices.some((d) => d.name === 'dropped' && d.kind === 'local')), 'a folder dropped into files/ is written to the file as a local device')

  // --- the file writes the web app -----------------------------------------------------------------------------------
  const c = readFile()
  c.mirrors = c.mirrors.filter((m) => m.name !== 'direct')
  c.mirrors.push({ name: 'second', remote: `second:${forward(secondDir)}`, keep_runs: 4, interval_hours: 6 })
  c.settings.retention_days = 33
  c.devices = c.devices.filter((d) => d.name !== 'tablet')
  writeFile(c)
  const edited = fs.readFileSync(configFile, 'utf8')
  await new Promise((r) => setTimeout(r, 3000))
  check((await admin.get('/api/settings')).json.retention_days === 20, 'an edit of the file is not applied while the master runs')
  await admin.put('/api/settings', { version_count: 8, retention_days: 21 })
  await new Promise((r) => setTimeout(r, 3000))
  check(fs.readFileSync(configFile, 'utf8') === edited, 'and a change in the web app does not write over it')
  await master.stop()
  await master.start()
  admin = await master.admin()
  check((await admin.get('/api/settings')).json.retention_days === 33, 'a restart applies a setting edited in the file')
  const mirrors = (await admin.get('/api/mirrors')).json
  check(names(mirrors).sort().join() === 'second,vault', 'a mirror added to the file appears and a removed one is gone')
  check(mirrors.find((m) => m.name === 'second').detail.keep_runs === 4, 'with its settings')
  const tablet = (await admin.get('/api/devices')).json.find((d) => d.name === 'tablet')
  check(tablet?.revoked === true, 'a device taken out of the file is revoked, not deleted')
  check(await fileSays((x) => x.devices.find((d) => d.name === 'tablet')?.revoked === true), 'and the file says so')
  check(await until(() => fs.existsSync(path.join(secondDir, 'files', 'laptop', 'docs', 'a.txt')), 40, 300), 'the new mirror runs')

  // --- a mistake in the file stops the start and is left alone --------------------------------------------------------
  const good = fs.readFileSync(configFile, 'utf8')
  const broken = good.replace('"keep_runs": 4', '"keep_runs": 400')
  await master.stop()
  fs.writeFileSync(configFile, broken)
  const refused = await startOnce(master.dbUrl)
  check(refused.status !== 0 && /keep_runs/.test(refused.out), 'a file that cannot be applied stops the master at start, and says what is wrong')
  check(fs.readFileSync(configFile, 'utf8') === broken, 'and is not written over')
  fs.writeFileSync(configFile, good)
  await master.start()
  admin = await master.admin()
  check((await admin.get('/api/mirrors')).json.find((m) => m.name === 'second').detail.keep_runs === 4, 'the master starts again once the file is right')

  // --- set up again from the file on an empty database ------------------------------------------------------------------
  await master.stop()
  const fresh = await createSchema()
  try {
    master.dbUrl = fresh.url
    fs.rmSync(path.join(master.data, 'files', 'laptop'), { recursive: true, force: true })
    await master.start()
    admin = await master.admin()
    check(names((await admin.get('/api/mirrors')).json).sort().join() === 'second,vault', 'a master with an empty database has its mirrors back')
    check((await admin.get('/api/settings')).json.retention_days === 33, 'and its settings')
    check((await admin.get('/api/devices')).json.find((d) => d.name === 'tablet')?.revoked === true, 'and its devices, revoked ones included')
    fs.writeFileSync(path.join(docs, 'b.txt'), 'beta')
    client.c('sync')
    check(fs.existsSync(path.join(master.data, 'files', 'laptop', 'docs', 'b.txt')), 'the device keeps working with the token it had')
  } finally {
    await master.stop()
    await fresh.drop()
  }

} catch (e) {
  console.error(e)
  process.exitCode = 1
} finally {
  await h.cleanup()
}
console.log(`\nall ${passedChecks()} checks passed`)
