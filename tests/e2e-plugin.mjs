// End-to-end test of plugins: the real master starts, feeds and updates (FAFNIRX_PLUGINS) the real fafnirx-video service from a bundle in
// <data>/plugins, installs a bundle from a stand-in release (a local server), refuses damaged and wrong bundles, and passes the
// module's web app files on to the browser.
//
// ffmpeg is not needed: small stand-in programs answer for ffprobe (see e2e-video.mjs).
//   cd tests && npm install && node e2e-plugin.mjs
import { execFileSync } from 'node:child_process'
import crypto from 'node:crypto'
import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import zlib from 'node:zlib'
import { BIN, check, EXE, freePort, harness, MASTER, passedChecks, sleep, until } from './lib.mjs'

const VIDEO = path.join(BIN, `fafnirx-video${EXE}`)
const h = await harness('fafnirx-e2e-plugin-', { binaries: [MASTER, VIDEO] })
const { tmp, data, schema, master } = h
const platform = `${process.platform === 'win32' ? 'windows' : process.platform}-${process.arch === 'x64' ? 'x86_64' : 'aarch64'}`

// --- stand-ins for ffprobe and ffmpeg -------------------------------------------------------------------------------
const fakes = path.join(tmp, 'fakes')
fs.mkdirSync(fakes)
fs.writeFileSync(path.join(fakes, 'ffprobe.mjs'), `console.log(JSON.stringify({ streams: [{ codec_type: 'video', width: 1280, height: 720 }], format: { duration: '14.5' } }))\n`)
fs.writeFileSync(path.join(fakes, 'ffmpeg.mjs'), `import fs from 'node:fs'\nconst a = process.argv.slice(2)\nfs.writeFileSync(a[a.length - 1], 'x')\n`)
function program(name) {
  const script = path.join(fakes, `${name}.mjs`)
  if (process.platform === 'win32') {
    const cmd = path.join(fakes, `${name}.cmd`)
    fs.writeFileSync(cmd, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`)
    return cmd
  }
  const sh = path.join(fakes, name)
  fs.writeFileSync(sh, `#!/bin/sh\nexec "${process.execPath}" "${script}" "$@"\n`, { mode: 0o755 })
  return sh
}

// --- bundles ----------------------------------------------------------------------------------------------------------
/** A zip file (stored, not compressed) of `{ name: Buffer }`. */
function zip(files) {
  const parts = []
  const central = []
  let offset = 0
  for (const [name, content] of Object.entries(files)) {
    const n = Buffer.from(name)
    const crc = zlib.crc32(content)
    const local = Buffer.alloc(30)
    local.writeUInt32LE(0x04034b50, 0)
    local.writeUInt16LE(20, 4)
    local.writeUInt32LE(crc, 14)
    local.writeUInt32LE(content.length, 18)
    local.writeUInt32LE(content.length, 22)
    local.writeUInt16LE(n.length, 26)
    parts.push(local, n, content)
    const entry = Buffer.alloc(46)
    entry.writeUInt32LE(0x02014b50, 0)
    entry.writeUInt16LE(0x031e, 4) // made on Unix
    entry.writeUInt16LE(20, 6)
    entry.writeUInt32LE(crc, 16)
    entry.writeUInt32LE(content.length, 20)
    entry.writeUInt32LE(content.length, 24)
    entry.writeUInt16LE(n.length, 28)
    entry.writeUInt32LE(0o100755 * 65536, 38)
    entry.writeUInt32LE(offset, 42)
    central.push(entry, n)
    offset += 30 + n.length + content.length
  }
  const directory = Buffer.concat(central)
  const end = Buffer.alloc(22)
  end.writeUInt32LE(0x06054b50, 0)
  end.writeUInt16LE(central.length / 2, 8)
  end.writeUInt16LE(central.length / 2, 10)
  end.writeUInt32LE(directory.length, 12)
  end.writeUInt32LE(offset, 16)
  return Buffer.concat([...parts, directory, end])
}

const exec = `bin/fafnirx-video${EXE}`
const bundleFiles = (version, { id = 'video' } = {}) => ({
  'plugin.json': Buffer.from(
    JSON.stringify({
      id,
      version,
      contract: 1,
      platform,
      exec,
      env: { FAFNIRX_FFMPEG: program('ffmpeg'), FAFNIRX_FFPROBE: program('ffprobe'), FAFNIRX_MODULE_UI: '{dir}/ui' },
    }),
  ),
  [exec]: fs.readFileSync(VIDEO),
  'ui/index.js': Buffer.from(`export default { marker: 'plugin-ui-${version}' }\n`),
  'ui/style.css': Buffer.from('.video-module{}\n'),
})
const bundle = (version, options) => zip(bundleFiles(version, options))

// --- a stand-in for the releases --------------------------------------------------------------------------------------
const VERSION = fs.readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8').match(/^version = "(.+)"/m)[1] // the plugin must match the master's own
const release = { version: VERSION, zip: bundle(VERSION), sha: null, missing: false }
const sha = (buffer) => crypto.createHash('sha256').update(buffer).digest('hex')
const port = await freePort()
const asset = `fafnirx-plugin-video-${release.version}-${platform}.zip`
const server = http.createServer((req, res) => {
  const url = `http://127.0.0.1:${port}`
  if (req.url === '/release') {
    const assets = release.missing ? [] : [asset, `${asset}.sha256`].map((name) => ({ name, browser_download_url: `${url}/${name}` }))
    res.setHeader('content-type', 'application/json')
    return res.end(JSON.stringify({ tag_name: `v${release.version}`, assets }))
  }
  if (req.url === `/${asset}`) return res.end(release.zip)
  if (req.url === `/${asset}.sha256`) return res.end(`${release.sha ?? sha(release.zip)}  ${asset}\n`)
  res.statusCode = 404
  res.end()
})
await new Promise((r) => server.listen(port, '127.0.0.1', r))

const running = () => {
  try {
    const out =
      process.platform === 'win32'
        ? execFileSync('tasklist', ['/FI', 'IMAGENAME eq fafnirx-video.exe', '/NH'], { encoding: 'utf8' })
        : execFileSync('pgrep', ['-x', 'fafnirx-video'], { encoding: 'utf8' })
    return /fafnirx-video/.test(out) || /^\d+/m.test(out)
  } catch {
    return false // pgrep finds none
  }
}
const modules = async (admin) => (await admin.get('/api/session')).json.modules ?? {}
const plugin = async (admin) => (await admin.get('/api/plugins')).json.installed.find((p) => p.id === 'video')

try {
  master.env = { FAFNIRX_PLUGIN_RELEASE_URL: `http://127.0.0.1:${port}/release` }

  // --- 1. A bundle that is unpacked into the folder by hand is started, found and used -------------------------------
  const folder = path.join(data, 'plugins', 'video')
  for (const [name, content] of Object.entries(bundleFiles('3.11.0'))) {
    fs.mkdirSync(path.dirname(path.join(folder, name)), { recursive: true })
    fs.writeFileSync(path.join(folder, name), content, { mode: 0o755 })
  }
  const dir = path.join(data, 'files', 'users', 'admin', 'videos')
  fs.mkdirSync(dir, { recursive: true })
  fs.writeFileSync(path.join(dir, 'clip.mp4'), 'not really a video')

  await master.start()
  const admin = await master.admin()
  check(await until(async () => (await modules(admin)).video, 40, 300), 'the master starts the plugin and the module is up')
  const up = (await modules(admin)).video
  check(typeof up.version === 'string' && up.ui?.entry === 'index.js' && up.ui?.css === 'style.css' && up.ui?.sdk === 1, 'its manifest tells the web app where its script and style are')
  check((await plugin(admin))?.state === 'up' && (await plugin(admin)).version === '3.11.0', 'the plugins page shows it running, with its version')

  const script = await admin.get('/api/m/video/ui/index.js')
  check(script.status === 200 && script.text.includes('plugin-ui-3.11.0'), "the module's script reaches the browser through the master")
  const css = await admin.bytes('/api/m/video/ui/style.css')
  check(css.status === 200 && css.headers.get('content-type').startsWith('text/css'), 'and its style, as CSS')
  check((await fetch(`${master.base}/api/m/video/ui/index.js`)).status === 401, 'both need a login')
  for (const bad of ['plugin.json', '..%2fplugin.json', '.env', 'nothing.js']) {
    check((await admin.get(`/api/m/video/ui/${bad}`)).status === 404, `/ui/${bad} is not served`)
  }
  check((await admin.get('/api/m/nothing/ui/index.js')).status === 404, 'a module that is not there has no files')

  // The module is used like any other: a new video gets its details from it.
  await admin.post('/api/devices', { name: 'videos', kind: 'local' })
  check(await until(async () => (await schema.rows("SELECT 1 FROM file_meta WHERE kind = 'video' AND duration_ms = 14500")).length, 40, 300), 'the master reads a new video through the plugin')

  // The plugins are the server administrator's: the web app only shows them.
  await admin.post('/api/users', { name: 'bob', role: 'normal', password: 'pw-bob-1234' })
  const bob = await master.admin('bob', 'pw-bob-1234')
  check((await bob.get('/api/plugins')).status === 403, 'only an administrator sees the plugins')
  check((await bob.get('/api/m/video/ui/index.js')).status === 200, 'any user gets the files of a module that is up')
  for (const [method, url] of [['post', '/api/plugins/video'], ['delete', '/api/plugins/video'], ['get', '/api/plugins/available']]) {
    check((await admin[method](url)).status >= 400, `the web app cannot change them: ${method} ${url}`)
  }
  check(fs.existsSync(folder) && (await plugin(admin)).version === '3.11.0', 'and nothing changed')

  // --- 2. FAFNIRX_PLUGINS installs from the release when the master starts; a bad one changes nothing ---------------
  const restart = async (env) => {
    await master.stop()
    master.env = { ...master.env, ...env }
    await master.start()
    return master.admin()
  }
  release.zip = bundle(VERSION)
  release.sha = 'f'.repeat(64)
  let again = await restart({ FAFNIRX_PLUGINS: 'video' })
  await sleep(3000)
  check((await plugin(again)).version === '3.11.0', 'a download that does not match its checksum is refused, and the plugin that is installed stays')
  release.sha = null

  release.zip = zip({ ...bundleFiles(VERSION), 'plugin.json': Buffer.from(JSON.stringify({ id: 'docs', version: VERSION, contract: 1, platform, exec })) })
  again = await restart({})
  await sleep(3000)
  check((await plugin(again)).version === '3.11.0' && (await again.get('/api/m/video/ui/index.js')).text.includes('3.11.0'), 'a bundle that is another plugin is refused, and it changes nothing')
  check(!fs.readdirSync(path.join(data, 'plugins')).some((n) => n.startsWith('.')), 'and leaves nothing behind')

  release.zip = bundle(VERSION)
  again = await restart({})
  check(await until(async () => (await plugin(again))?.state === 'up' && (await plugin(again)).version === VERSION, 40, 300), 'a good bundle installs, and the new version runs in its place')
  check(await until(async () => (await again.get('/api/m/video/ui/index.js')).text?.includes(`plugin-ui-${VERSION}`), 40, 300), 'with its own script')
  check((await schema.rows("SELECT 1 FROM file_meta WHERE kind = 'video'")).length === 1, 'the files stay')

  // --- 3. Deleting the folder takes the module away -----------------------------------------------------------------
  fs.rmSync(path.join(folder, 'plugin.json')) // Windows keeps a running program's folder: it is the file that makes the folder a plugin
  check(await until(async () => !(await modules(again)).video, 20, 300), 'with its folder emptied of plugin.json the module is not there any more')
  check(await until(async () => !running(), 20, 250), 'and its program is stopped')
  fs.rmSync(folder, { recursive: true, force: true })

  // --- 4. Install from nothing, then the program goes with the master -----------------------------------------------
  again = await restart({})
  check(await until(async () => (await modules(again)).video, 40, 300), 'FAFNIRX_PLUGINS installs a plugin from the release when nothing is there, and it starts')
  check(running(), 'its program is running')
  await master.stop()
  check(await until(async () => !running(), 20, 250), 'when the master stops, the programs it started stop')

  // --- 5. A module set up by address wins ---------------------------------------------------------------------------
  master.env = { ...master.env, FAFNIRX_MODULE_VIDEO_URL: 'http://127.0.0.1:9', FAFNIRX_MODULE_VIDEO_TOKEN: 'a-token-that-is-long-enough' }
  await master.start()
  const external = await master.admin()
  await sleep(4000)
  const listed = await plugin(external)
  check(!running() && listed?.state === 'external', 'a module named by address is used instead of the plugin: the plugin is not started')
} catch (e) {
  console.error(e)
  process.exitCode = 1
} finally {
  server.close()
  await h.cleanup()
}
console.log(`\n${passedChecks()} checks passed`)
