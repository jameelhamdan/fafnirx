// End-to-end test: the real master, the real web API and the real client binary.
//   cd tests && npm install && node e2e.mjs
import { spawn } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { CHUNK, CLIENT, ENV, EXE, MASTER, SIGN, check, clientIn, harness, passedChecks, run, sleep, until } from './lib.mjs'

const h = await harness('fafnirx-e2e-', { binaries: [MASTER, CLIENT, SIGN] })
const { tmp, data, schema, master } = h
const server = `http://127.0.0.1:${master.grpc}`
let daemon = null

/** What the master stores for a path of the device: the live file, or its folder of the version once deleted. */
const stored = async (device, p) => {
  const [row] = await schema.rows(
    "SELECT f.current_version_id, f.deleted_at FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = $1 AND f.path = $2",
    [device, p],
  )
  if (!row) return null
  const file = row[1] === null ? path.join(data, 'files', device, p) : path.join(data, 'versions', String(row[0]), device, p)
  return fs.existsSync(file) ? fs.readFileSync(file) : null
}
const known = async (p) => (await schema.rows('SELECT 1 FROM files WHERE path = $1', [p])).length > 0

try {
  await master.start()
  const admin = await master.admin()

  // --- the web API creates the device ---------------------------------------------------------
  const made = await admin.post('/api/devices', { name: 'laptop' })
  const token = made.json?.token
  check(made.status === 200 && token?.length >= 40, 'device created through the web API, token shown once')
  check((await admin.get('/api/devices/admin%2Flaptop')).json.token === null, 'the token is not shown again')
  check(fs.existsSync(path.join(data, 'files', 'admin', 'laptop')), "the device's folder exists from the start, named after it")

  // --- enroll the client over plain http -------------------------------------------------------------
  const cdir = path.join(tmp, 'client')
  const { cfg, c } = clientIn(cdir)
  const out = c.enroll(token, server).stdout
  check(out.includes('as device "admin/laptop"'), `enroll connects without TLS (${out.trim().split('\n').pop()})`)
  fs.writeFileSync(cfg, `state_dir = "${path.join(tmp, 'state').replaceAll('\\', '/')}"\n${fs.readFileSync(cfg, 'utf8')}`)
  // The enrolling above ran with the default state directory, which counts as one computer; the client of this test keeps
  // its state elsewhere, so it is another. Releasing the device lets that one take it (and exercises the release).
  check((await admin.post('/api/devices/admin%2Flaptop/release')).status === 200, 'a device can be released for another computer')
  const plain = run(CLIENT,['--config', path.join(tmp, 'x', 'config.toml'), 'enroll', '--no-service', '--server', 'http://203.0.113.5:7443'], { ok: false, input: 'x\n' })
  check(plain.stdout.includes('plain http on a public address'), 'plain http to a public address is warned about')

  // --- a client that cannot start says why, where the tray looks ----------------------------------------------
  const lone = clientIn(path.join(tmp, 'notoken'))
  fs.mkdirSync(path.join(tmp, 'notoken'), { recursive: true })
  const loneState = path.join(tmp, 'notoken-state').replaceAll('\\', '/')
  fs.writeFileSync(lone.cfg, `server = "${server}"\nstate_dir = "${loneState}"\n`)
  check(lone.c.try('run').code !== 0, 'a client without a token does not start')
  check(lone.c('status').stdout.includes('no device token'), 'and the status says why, so the tray can show it')

  // --- first collection --------------------------------------------------------------------------------
  const src = path.join(tmp, 'docs')
  fs.mkdirSync(path.join(src, 'sub', 'deep'), { recursive: true })
  const big = randomBytes(CHUNK * 2 + 12345)
  fs.writeFileSync(path.join(src, 'big.bin'), big)
  fs.writeFileSync(path.join(src, 'sub', 'a.txt'), 'hello')
  fs.writeFileSync(path.join(src, 'sub', 'deep', 'b.txt'), 'world')
  fs.writeFileSync(path.join(src, 'skip.tmp'), 'ignored')
  fs.writeFileSync(path.join(src, '.fafnirxignore'), '*.tmp\n')
  fs.writeFileSync(path.join(src, 'empty.txt'), '')
  c('add-folder', src, '--name', 'docs')
  const first = c('sync').stdout
  check(first.includes('5 uploaded'), `first sync uploads 5 files (${first.trim()})`)

  const paths = new Set((await schema.rows('SELECT path FROM files WHERE deleted_at IS NULL')).map((r) => r[0]))
  check(
    ['docs/big.bin', 'docs/sub/a.txt', 'docs/sub/deep/b.txt', 'docs/.fafnirxignore', 'docs/empty.txt'].every((p) => paths.has(p)) && paths.size === 5,
    `the device holds the expected paths ${[...paths].sort()}`,
  )
  check((await stored('admin/laptop', 'docs/big.bin'))?.equals(big), 'a multi-chunk file is stored as one normal file, byte for byte')
  check((await stored('admin/laptop', 'docs/sub/a.txt'))?.toString() === 'hello', 'a small file is stored under its own name')

  const listing = (await admin.get('/api/files?device=admin/laptop&path=docs')).json
  check(listing.entries.some((e) => e.kind === 'dir' && e.name === 'sub') && listing.entries.some((e) => e.name === 'big.bin'), 'the file browser lists what was collected')
  const dl = await admin.bytes('/api/files/download?device=admin/laptop&path=docs/big.bin')
  check(dl.status === 200 && dl.body.equals(big), 'the web API downloads a multi-chunk file intact')
  const part = await admin.bytes('/api/files/download?device=admin/laptop&path=docs/big.bin', { range: `bytes=${CHUNK}-${CHUNK + 99}` })
  check(part.status === 206 && part.body.equals(big.subarray(CHUNK, CHUNK + 100)), 'a byte range of a download is served (streaming, resume)')

  // --- unchanged: nothing is sent ---------------------------------------------------------------------------
  const again = c('sync').stdout
  check(again.includes('0 uploaded') && again.includes('0 bytes sent'), 'a second sync sends nothing')

  // --- the master is replaced by an empty one: the client notices and sends everything again ---------------------
  await master.stop()
  await schema.rows('DELETE FROM events')
  await schema.rows('DELETE FROM versions')
  await schema.rows('DELETE FROM files')
  await schema.rows("UPDATE settings SET value = 'a-different-master' WHERE key = 'master_id'")
  fs.rmSync(path.join(data, 'files', 'admin', 'laptop'), { recursive: true })
  fs.mkdirSync(path.join(data, 'files', 'admin', 'laptop'))
  await master.start()
  const again2 = c('sync').stdout
  check(again2.includes('5 uploaded') && !again2.includes('0 bytes sent'), `a new master gets everything again, not "nothing changed" (${again2.trim()})`)
  check((await stored('admin/laptop', 'docs/big.bin'))?.equals(big), 'and the files are really on it')

  // --- edit the big file: it is sent whole and the old copy moves to versions/ ------------------------------
  const edited = Buffer.from(big)
  edited[10] ^= 0xff
  fs.writeFileSync(path.join(src, 'big.bin'), edited)
  const sent = Number(/(\d+) bytes sent/.exec(c('sync').stdout)[1])
  check(sent === edited.length, `an edited file is sent whole (${sent} bytes)`)
  const info = (await admin.get('/api/files/info?device=admin/laptop&path=docs/big.bin')).json
  check(info.versions.length === 2 && info.versions.every((v) => v.available), 'the old version is kept next to the new one')
  const old = info.versions.find((v) => !v.current)
  const oldBytes = await admin.bytes(`/api/files/download?device=admin/laptop&path=docs/big.bin&version=${old.id}`)
  check(oldBytes.body.equals(big), 'an older version can be downloaded')
  check((await stored('admin/laptop', 'docs/big.bin')).equals(edited), 'the current version is the edited file')

  // --- dedup across paths -----------------------------------------------------------------------------------------
  fs.copyFileSync(path.join(src, 'big.bin'), path.join(src, 'copy-of-big.bin'))
  check(c('sync').stdout.includes('0 bytes sent'), 'a copy of existing content uploads no data (dedup)')

  // --- rename ----------------------------------------------------------------------------------------------------
  fs.renameSync(path.join(src, 'sub', 'a.txt'), path.join(src, 'sub', 'renamed.txt'))
  const renamed = c('sync').stdout
  check(renamed.includes('1 renamed') && renamed.includes('0 bytes sent'), `a rename is detected without re-uploading (${renamed.trim()})`)
  const [ev] = await schema.rows("SELECT action, detail FROM events WHERE path = 'docs/sub/renamed.txt'")
  check(ev?.[0] === 'rename' && ev?.[1] === 'docs/sub/a.txt', 'the rename is recorded in the change log')

  // --- a file becomes a folder of the same name: the old path is deleted first, so the new files can be stored ----
  fs.writeFileSync(path.join(src, 'swap'), 'a file')
  c('sync')
  fs.rmSync(path.join(src, 'swap'))
  fs.mkdirSync(path.join(src, 'swap'))
  fs.writeFileSync(path.join(src, 'swap', 'inside.txt'), 'now a folder')
  check(c.try('sync').code === 0 && (await stored('admin/laptop', 'docs/swap/inside.txt'))?.toString() === 'now a folder', 'a file replaced by a folder does not wedge the client')
  fs.rmSync(path.join(src, 'swap'), { recursive: true })
  c('sync')

  // --- a collected folder that is gone (an unmounted drive, a mistake) leaves what the master holds alone ----------
  const held = path.join(tmp, 'held')
  fs.mkdirSync(held)
  fs.writeFileSync(path.join(held, 'keep.txt'), 'keep me')
  c('add-folder', held, '--name', 'held')
  c('sync')
  fs.rmSync(held, { recursive: true })
  c.try('sync')
  check((await stored('admin/laptop', 'held/keep.txt'))?.toString() === 'keep me' && (await schema.rows("SELECT 1 FROM files WHERE path = 'held/keep.txt' AND deleted_at IS NULL")).length === 1, 'files of a collected folder that disappeared stay on the master')
  c('remove-folder', 'held')

  // --- deleted in the web app: hidden on the master, not sent again, and the device's file is left alone ----------
  const mine = path.join(src, 'web-deleted.txt')
  fs.writeFileSync(mine, 'v1')
  c('sync')
  await admin.post('/api/files/delete', { items: [{ device: 'admin/laptop', path: 'docs/web-deleted.txt' }] })
  fs.writeFileSync(mine, 'v2, changed on the device after the delete')
  c('sync')
  const [webDeleted] = await schema.rows("SELECT deleted_at, deleted_by FROM files WHERE path = 'docs/web-deleted.txt'")
  check(webDeleted?.[0] != null && webDeleted[1] === 'web' && (await stored('admin/laptop', 'docs/web-deleted.txt'))?.toString() === 'v1', 'a file deleted in the web app stays deleted and is not sent again')
  check(fs.readFileSync(mine, 'utf8').startsWith('v2'), 'and the file on the device is untouched')
  fs.rmSync(mine)
  c('sync')

  // --- delete: the data stays --------------------------------------------------------------------------------------
  fs.unlinkSync(path.join(src, 'sub', 'deep', 'b.txt'))
  check(c('sync').stdout.includes('1 deleted'), 'a delete is reported')
  const [[deletedAt]] = await schema.rows("SELECT deleted_at FROM files WHERE path = 'docs/sub/deep/b.txt'")
  check(deletedAt !== null, 'the path is marked deleted')
  check((await stored('admin/laptop', 'docs/sub/deep/b.txt'))?.toString() === 'world', 'a deleted file is still fully recoverable')
  check(!(await admin.get('/api/files?device=admin/laptop&path=docs/sub/deep')).json.entries.some((e) => e.name === 'b.txt'), 'a deleted file leaves the listing')
  const gone = (await admin.get('/api/files?device=admin/laptop&path=docs/sub/deep&deleted=1')).json.entries.find((e) => e.name === 'b.txt')
  check(gone?.deleted_at && gone.purge_at > gone.deleted_at, 'with "deleted" on, it shows when it will be removed for good')

  // --- resume after the client is killed mid-upload -------------------------------------------------------------------
  const huge = randomBytes(CHUNK * 6)
  fs.writeFileSync(path.join(src, 'huge.bin'), huge)
  const partialSize = () => fs.readdirSync(path.join(data, 'tmp')).filter((f) => f.startsWith('up-')).reduce((n, f) => n + fs.statSync(path.join(data, 'tmp', f)).size, 0)
  fs.writeFileSync(cfg, fs.readFileSync(cfg, 'utf8').replace('bandwidth_limit_kbps = 0', 'bandwidth_limit_kbps = 2048'))
  const slow = spawn(CLIENT, ['--config', cfg, 'sync'], { env: ENV, stdio: 'ignore' })
  await sleep(5000)
  slow.kill()
  const partial = partialSize()
  check(partial > 0, `the upload was interrupted part way (${partial} bytes of the partial file on disk)`)
  check(!(await known('docs/huge.bin')), 'a killed upload leaves no half-written file visible')
  fs.writeFileSync(cfg, fs.readFileSync(cfg, 'utf8').replace('bandwidth_limit_kbps = 2048', 'bandwidth_limit_kbps = 0'))
  const resumed = Number(/(\d+) bytes sent/.exec(c('sync').stdout)[1])
  check(resumed < huge.length, `resuming re-sends only the missing chunks (${resumed} of ${huge.length} bytes)`)
  check((await stored('admin/laptop', 'docs/huge.bin')).equals(huge), 'the resumed file is complete and identical')

  // --- the master is down: changes queue, then flow when it returns --------------------------------------------------------
  await master.stop()
  fs.writeFileSync(path.join(src, 'while-offline.txt'), 'written while the master was down')
  const failed = c.try('sync')
  check(failed.code !== 0 && failed.stderr.includes('sync failed'), 'sync reports an error while the master is unreachable')
  await master.start()
  check(c('sync').stdout.includes('1 uploaded'), 'the queued change uploads once the master is back')
  const live = (await (await master.admin()).get('/api/devices')).json.find((d) => d.name === 'laptop')
  check(live.files >= 6 && live.bytes > 0, 'the web API shows what was collected for the device')

  // --- the daemon: watcher uploads, pause and resume, status ------------------------------------------------------------------
  daemon = spawn(CLIENT, ['--config', cfg, 'run'], { env: ENV, stdio: 'ignore' })
  const online = await until(() => c('status').stdout.split('\n').slice(0, 2).join(' ').includes('online'), 30)
  check(online, 'the daemon connects and reports online')
  const admin2 = await master.admin()
  check((await admin2.get('/api/devices')).json.find((d) => d.name === 'laptop').online === true, 'the web API shows the device online')
  // --- a second computer with the same token is refused, and tells why ---------------------------------------------------
  const twinDir = path.join(tmp, 'twin')
  const twin = clientIn(twinDir)
  const twinOut = twin.c.enroll(token, server).stdout
  check(twinOut.includes('device in use'), `a second computer enrolling with the same token is told the device is in use (${twinOut.trim().split('\n').pop()})`)
  fs.writeFileSync(twin.cfg, `state_dir = "${path.join(tmp, 'twin-state').replaceAll('\\', '/')}"\n${fs.readFileSync(twin.cfg, 'utf8')}`)
  const twinSrc = path.join(tmp, 'twin-docs')
  fs.mkdirSync(twinSrc, { recursive: true })
  fs.writeFileSync(path.join(twinSrc, 'twin-only.txt'), 'must never arrive')
  twin.c('add-folder', twinSrc, '--name', 'twin')
  const twinDaemon = spawn(CLIENT, ['--config', twin.cfg, 'run'], { env: ENV, stdio: 'ignore' })
  check(await until(() => twin.c('status').stdout.includes('not sending'), 40), 'the second computer waits and says it is not sending')
  await new Promise((r) => setTimeout(r, 3000))
  check(!(await known('twin/twin-only.txt')), 'and nothing it has is sent')
  check(c('status').stdout.split('\n').slice(0, 2).join(' ').includes('online'), 'the first computer carries on undisturbed')
  check((await admin2.get('/api/dashboard')).json.alerts.some((a) => a.kind === 'device_conflict' && a.key === 'admin/laptop'), 'the owner gets an alert about the second computer')
  twinDaemon.kill()
  const second = spawn(CLIENT, ['--config', cfg, 'run'], { env: ENV, stdio: ['ignore', 'pipe', 'ignore'] })
  let said = ''
  second.stdout.on('data', (d) => (said += d))
  const exited = await until(() => second.exitCode !== null, 20)
  if (!exited) second.kill()
  check(exited && said.includes('already running'), 'a second client for the same config exits at once (only one runs)')
  const late = path.join(tmp, 'late')
  fs.mkdirSync(path.join(late, 'inner'), { recursive: true })
  fs.writeFileSync(path.join(late, 'inner', 'first.txt'), 'a folder added while the client runs')
  c('add-folder', late, '--name', 'late')
  check(await until(() => known('late/inner/first.txt'), 30), 'a folder added while the client runs is collected at once, not at the next scan')
  fs.writeFileSync(path.join(src, 'watched.txt'), 'picked up by the watcher')
  check(await until(() => known('docs/watched.txt'), 30), 'the file watcher uploads a new file')
  c('pause')
  await sleep(1000)
  fs.writeFileSync(path.join(src, 'while-paused.txt'), 'not yet')
  await sleep(6000)
  check(!(await known('docs/while-paused.txt')), 'nothing uploads while paused')
  check(c('status').stdout.includes('paused'), 'status shows paused')
  c('resume')
  check(await until(() => known('docs/while-paused.txt'), 30), 'uploading continues after resume')
  daemon.kill()
  daemon = null

  // --- the client follows a new enrollment without being restarted by hand -----------------------------------------------------------
  const rotated = (await admin2.post('/api/devices/admin%2Flaptop/token')).json.token
  check(rotated && rotated !== token, 'a rotated token is different')
  fs.writeFileSync(path.join(src, 'after-rotate.txt'), 'sent with the new token')
  check(c.try('sync').code !== 0, 'the old token no longer works')
  c.enroll(rotated, server)
  check(c('sync').stdout.includes('1 uploaded'), 'enrolling again with the new token works')

  // --- a revoked device is locked out -----------------------------------------------------------------------------------------------
  fs.writeFileSync(path.join(src, 'after-revoke.txt'), 'x')
  await admin2.post('/api/devices/admin%2Flaptop/revoke')
  check(c.try('sync').code !== 0, 'a revoked device cannot upload')
  check(!(await known('docs/after-revoke.txt')), 'nothing was stored for the revoked device')

  // --- a local folder: upload in the browser's chunks, then files dropped straight into it ------------------------------------------------
  check((await admin2.post('/api/devices', { name: 'photos', kind: 'local' })).status === 200, 'a local folder is created')
  const photo = randomBytes(300_000)
  const upload = (await admin2.post('/api/uploads', { device: 'admin/photos', path: '2024/p.jpg', size: photo.length })).json
  for (let at = 0; at < photo.length; at += 100_000) {
    const r = await fetch(`${master.base}/api/uploads/${upload.id}?offset=${at}`, { method: 'PUT', headers: { cookie: admin2.cookie, 'x-requested-with': 'e2e' }, body: photo.subarray(at, at + 100_000) })
    if (!r.ok) check(false, `an upload chunk is accepted (${r.status})`)
  }
  check((await admin2.post(`/api/uploads/${upload.id}/finish`)).status === 200, 'a chunked upload finishes')
  check((await stored('admin/photos', '2024/p.jpg'))?.equals(photo), 'an uploaded file is stored under files/<name>/ byte for byte')
  fs.mkdirSync(path.join(data, 'files', 'admin', 'photos', 'dropped'), { recursive: true })
  fs.writeFileSync(path.join(data, 'files', 'admin', 'photos', 'dropped', 'x.txt'), 'copied straight into the share')
  check(await until(() => known('dropped/x.txt'), 60), 'a file dropped into files/<name>/ is picked up (file watcher)')
  fs.mkdirSync(path.join(data, 'files', 'admin', 'newdrive'))
  fs.writeFileSync(path.join(data, 'files', 'admin', 'newdrive', 'y.txt'), 'in a folder nobody created')
  check(await until(() => known('y.txt'), 60), 'a new folder in the folder of a user becomes a device of that user')
  const kind = (await schema.rows("SELECT kind FROM devices WHERE name = 'newdrive'"))[0]?.[0]
  check(kind === 'local', 'and it is a local device')
  // Master files: whatever is in files/ and is not the folder of a user belongs to nobody.
  fs.mkdirSync(path.join(data, 'files', 'scans'), { recursive: true })
  fs.writeFileSync(path.join(data, 'files', 'scans', 'loose.txt'), 'dropped on the share, for everybody')
  check(await until(() => known('scans/loose.txt'), 60), 'a file dropped into files/ outside any user folder is picked up')
  const [master0] = await schema.rows("SELECT d.kind, d.user_id IS NULL, d.folder FROM files f JOIN devices d ON d.id = f.device_id WHERE f.path = 'scans/loose.txt'")
  check(master0?.[0] === 'master' && master0[1] === true && master0[2] === '_master', 'it is a master file: no user, no device of anyone')
  check((await schema.rows("SELECT 1 FROM files WHERE device_id = 0 AND path LIKE 'admin/%'")).length === 0, 'the folder of a user is never part of the master files')
  const bob = (await admin2.post('/api/users', { name: 'bob', role: 'readonly', password: 'password-1' }))
  check(bob.status === 200, 'a user is added')
  const asBob = await master.admin('bob', 'password-1')
  check((await asBob.get('/api/files?device=_master&path=scans')).json.entries.some((e) => e.name === 'loose.txt'), 'a read-only user sees the master files')
  check((await asBob.bytes('/api/files/download?device=_master&path=scans/loose.txt')).body.toString() === 'dropped on the share, for everybody', 'and can download them')
  check((await asBob.get('/api/files?device=admin/photos')).status === 404, 'but not the files of the administrator')
  check((await asBob.post('/api/devices', { name: 'mine' })).status === 403, 'and has no devices of their own')
  // The top level says what each folder is and whether the web app may change it.
  const tops = async (who) => Object.fromEntries((await who.get('/api/files')).json.entries.map((e) => [e.device, [e.device_kind, e.writable]]))
  const adminTop = await tops(admin2)
  check(adminTop._master?.join() === 'master,true' && adminTop['admin/photos']?.join() === 'local,true', 'the master files and a local folder are changeable')
  check(adminTop['admin/laptop']?.join() === 'device,false', 'what a computer sends is read-only in the web app')
  check((await tops(asBob))._master?.join() === 'master,false', 'a read-only user cannot change the master files')
  // A scan can be asked for, by an administrator only.
  const oldFile = path.join(data, 'files', 'scans', 'asked-for.txt')
  fs.writeFileSync(oldFile, 'put there by hand')
  fs.utimesSync(oldFile, new Date(Date.now() - 120000), new Date(Date.now() - 120000))
  check((await asBob.post('/api/files/rescan')).status === 403, 'only an administrator can ask for a scan')
  const asked = await admin2.post('/api/files/rescan')
  check(asked.status === 200 && typeof asked.json.started === 'boolean', 'an administrator can ask for a scan')
  check(await until(() => known('scans/asked-for.txt'), 60), 'and the file put there by hand is recorded')
  fs.unlinkSync(path.join(data, 'files', 'admin', 'photos', 'dropped', 'x.txt'))
  const removed = await until(async () => (await schema.rows("SELECT deleted_at FROM files WHERE path = 'dropped/x.txt'"))[0]?.[0] != null, 60)
  check(removed, 'a file removed from files/<name>/ is marked deleted')

  // --- signed update -----------------------------------------------------------------------------------------------------------------
  const key = path.join(tmp, 'update.key')
  const pub = /update_public_key = "([^"]+)"/.exec(run(SIGN, ['keygen', '--out', key]).stdout)[1]
  const newToken = (await admin2.post('/api/devices/admin%2Flaptop/token')).json.token
  const copy = path.join(tmp, `fafnirx-copy${EXE}`)
  fs.copyFileSync(CLIENT, copy)
  const before = fs.readFileSync(copy)
  fs.writeFileSync(cfg, `update_public_key = "${pub}"\n${fs.readFileSync(cfg, 'utf8')}`)
  const platform = `${{ win32: 'windows', darwin: 'macos', linux: 'linux' }[process.platform]}-x86_64`
  run(SIGN, ['publish', '--key', key, '--platform', platform, '--version', '99.0.0', '--bin', SIGN, '--out', path.join(data, 'updates')])
  run(CLIENT, ['--config', cfg, 'enroll', '--no-service', '--server', server], { input: `${newToken}\n` })
  const sig = path.join(data, 'updates', platform, 'signature')
  const good = fs.readFileSync(sig, 'utf8')
  fs.writeFileSync(sig, good.slice(0, 10) + (good[10] === 'A' ? 'B' : 'A') + good.slice(11))
  const refused = run(copy, ['--config', cfg, 'update'], { ok: false })
  check(refused.code !== 0 && fs.readFileSync(copy).equals(before), 'a tampered update is refused and the binary is untouched')
  fs.writeFileSync(sig, good)
  const applied = run(copy, ['--config', cfg, 'update'], { ok: false })
  check(applied.code === 0 && fs.readFileSync(copy).equals(fs.readFileSync(SIGN)), `a signed update replaces the executable (${applied.stdout.trim()})`)

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  daemon?.kill()
  await h.cleanup()
}
