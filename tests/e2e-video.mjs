// End-to-end test of the video module: the real master in front of the real fafnirx-video service.
//
// ffmpeg is not needed: small stand-in programs answer for ffprobe and ffmpeg, so that what is tested is everything
// around them: the manifest the master asks for, the web login in front, the token between the two services, the
// details and poster frame the master gets for a new video, the playlists, which stretch of the video each segment asks
// ffmpeg for, that a segment is made once and kept, and that the service refuses paths that leave the file tree. (Real
// transcoding needs ffmpeg and is checked by hand: docs/video.md.)
//   cd tests && npm install && node e2e-video.mjs
import { spawn } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { BIN, check, ENV, EXE, freePort, harness, MASTER, passedChecks, until } from './lib.mjs'

const VIDEO = path.join(BIN, `fafnirx-video${EXE}`)
const TOKEN = 'module-token-for-the-test-0123456789'
const h = await harness('fafnirx-e2e-video-', { binaries: [MASTER, VIDEO] })
const { tmp, data, schema, master } = h

// --- stand-ins for ffprobe and ffmpeg -------------------------------------------------------------------------------
const fakes = path.join(tmp, 'fakes')
fs.mkdirSync(fakes)
const calls = path.join(tmp, 'ffmpeg-calls.log')
fs.writeFileSync(
  path.join(fakes, 'ffprobe.mjs'),
  `console.log(JSON.stringify({ streams: [{ codec_type: 'video', width: 1280, height: 720 }, { codec_type: 'audio' }], format: { duration: '14.5', tags: { creation_time: '2023-11-14T22:13:20.000000Z' } } }))\n`,
)
fs.writeFileSync(
  path.join(fakes, 'ffmpeg.mjs'),
  `import fs from 'node:fs'
const a = process.argv.slice(2)
const at = (flag) => (a.includes(flag) ? a[a.indexOf(flag) + 1] : undefined)
fs.appendFileSync(${JSON.stringify(calls)}, 'ss=' + at('-ss') + ' t=' + at('-t') + ' vf=' + at('-vf') + '\\n')
const out = a[a.length - 1]
const text = 'TS ss=' + at('-ss') + ' t=' + at('-t')
// A flag or '-' as the last word is no file: ffmpeg -version, and a frame sent to standard output.
if (out.startsWith('-')) process.stdout.write(text)
else fs.writeFileSync(out, text)\n`,
)
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

const videoPort = await freePort()
const videoUrl = `http://127.0.0.1:${videoPort}`
let service = null
const called = () => (fs.existsSync(calls) ? fs.readFileSync(calls, 'utf8').trim().split('\n').filter(Boolean) : [])
// A poster is one frame (no -t); a segment is a stretch of the video.
const calledSegments = () => called().filter((l) => !l.includes('t=undefined'))
const calledPosters = () => called().filter((l) => l.includes('t=undefined'))

try {
  const log = fs.openSync(path.join(tmp, 'video.log'), 'a')
  service = spawn(VIDEO, [], {
    stdio: ['ignore', log, log],
    env: {
      ...ENV,
      FAFNIRX_MODULE_TOKEN: TOKEN,
      FAFNIRX_DATA: data,
      FAFNIRX_VIDEO_CACHE: path.join(tmp, 'video-cache'),
      FAFNIRX_MODULE_LISTEN: `127.0.0.1:${videoPort}`,
      FAFNIRX_FFMPEG: program('ffmpeg'),
      FAFNIRX_FFPROBE: program('ffprobe'),
    },
  })
  check(await until(async () => { try { return (await fetch(`${videoUrl}/health`)).ok } catch { return false } }, 20, 100), 'the video module starts')

  master.env = { FAFNIRX_MODULE_VIDEO_URL: videoUrl, FAFNIRX_MODULE_VIDEO_TOKEN: TOKEN }
  await master.start()
  const admin = await master.admin()
  const up = await until(async () => (await admin.get('/api/session')).json.modules?.video, 30, 300)
  check(up?.id === 'video' && up.contract === 1 && up.capabilities.includes('poster'), 'the master asks the module for its manifest, and the web app is told that the module is up')

  // A video in a local folder.
  await admin.post('/api/devices', { name: 'videos', kind: 'local' })
  fs.mkdirSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip'), { recursive: true })
  fs.writeFileSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip', 'clip.mkv'), 'pretend this is a video')
  const id = await until(async () => (await schema.rows("SELECT id FROM files WHERE path = 'trip/clip.mkv'"))[0]?.[0], 30, 300)
  check(id, 'the master finds the video')

  // --- what the master asks the module for ------------------------------------------------------------------------------
  const details = await until(async () => (await schema.rows("SELECT duration_ms, width, height, taken_at FROM file_meta WHERE kind = 'video'"))[0], 30, 300)
  check(details && Number(details[0]) === 14500 && Number(details[1]) === 1280 && Number(details[2]) === 720 && Number(details[3]) === 1_700_000_000, 'the details of the video come from the module')
  const thumb = await until(async () => { const r = await admin.get(`/api/media/${id}/thumb`); return r.status === 200 ? r : null }, 30, 300)
  check(thumb?.text.startsWith('TS ss=1') && calledPosters().some((l) => l.includes('scale=min(320')), 'its poster frame is made by the module, as a picture of the size asked for')

  // --- the playlists ------------------------------------------------------------------------------------------------------
  const stream = (rest) => admin.get(`/api/m/video/${id}/hls/${rest}`)
  const top = await stream('master.m3u8')
  check(top.status === 200 && top.text.includes('720p/index.m3u8') && top.text.includes('360p/index.m3u8') && !top.text.includes('1080p'), 'the master playlist offers the qualities up to the video\'s own')
  const list = await stream('720p/index.m3u8')
  check(list.status === 200 && list.text.includes('seg2.ts') && !list.text.includes('seg3.ts') && list.text.includes('#EXTINF:2.500'), 'the quality playlist lists every segment, the last one shorter')
  check(calledSegments().length === 0, 'playlists need no transcoding')

  // --- the segments -------------------------------------------------------------------------------------------------------
  const seg1 = await stream('720p/seg1.ts')
  check(seg1.status === 200 && seg1.text === 'TS ss=6.000 t=6.000', 'segment 1 is made from second 6 for 6 seconds')
  const seg2 = await stream('720p/seg2.ts')
  check(seg2.text === 'TS ss=12.000 t=2.500', 'the last segment is the rest of the video')
  await stream('720p/seg1.ts')
  check(calledSegments().length === 2, 'a segment is made once and then kept')
  check(calledSegments()[0].includes('vf=scale=1280:720'), 'at the size of its quality')
  const low = await stream('360p/seg0.ts')
  check(low.status === 200 && calledSegments()[2].includes('vf=scale=640:360'), 'each quality has its own segments')
  check((await stream('720p/seg3.ts')).status === 404 && (await stream('1080p/index.m3u8')).status === 404, 'segments past the end and qualities not offered are not found')
  check((await stream('720p/seg1.mp4')).status === 404 && (await stream('..%2Fmaster.m3u8')).status === 404, 'only playlists and segments are passed on')

  // --- who may ask ------------------------------------------------------------------------------------------------------------
  const anonymous = await fetch(`${master.base}/api/m/video/${id}/hls/master.m3u8`)
  check(anonymous.status === 401, 'the master asks for a login first')
  const direct = (p, headers = {}) => fetch(`${videoUrl}/files/hls/master.m3u8?p=${p}&h=abc123`, { headers })
  check((await direct('users/admin/videos/trip/clip.mkv')).status === 401, 'the module itself refuses a request without the token')
  check((await direct('users/admin/videos/trip/clip.mkv', { authorization: 'Bearer wrong-token-wrong-token' })).status === 401, 'and with a wrong one')
  const ok = { authorization: `Bearer ${TOKEN}` }
  check((await direct('users/admin/videos/trip/clip.mkv', ok)).status === 200, 'and answers with the right one')
  check((await direct('videos/..%2F..%2Fsecret', ok)).status === 400 && (await direct('..%2Fx%2Fy', ok)).status === 400, 'the module refuses paths that leave the file tree')

  // What only the master may ask for is not reachable through its login: /files is all the web app gets.
  const status = await admin.get(`/api/m/video/${id}/status`)
  check(status.status === 404, 'the report and the manifest of the module are not served to the web app')
  const queue = (await admin.get('/api/queue')).json
  check(Array.isArray(queue.modules.video?.active), 'the queue page gets the module\'s report')

  // --- a file that is gone or changed ---------------------------------------------------------------------------------------------
  fs.writeFileSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip', 'clip.mkv'), 'a different video, another content hash')
  const changed = await until(async () => (await schema.rows("SELECT encode(v.root_hash, 'hex') FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'trip/clip.mkv' ORDER BY v.id")).length >= 2, 30, 300)
  check(changed, 'the master records the new content')
  const before = calledSegments().length
  check((await stream('720p/seg1.ts')).status === 200 && calledSegments().length === before + 1, 'changed content gets new segments, never stale ones')

  await admin.post('/api/files/delete', { items: [{ device: 'admin/videos', path: 'trip/clip.mkv' }] })
  check((await stream('master.m3u8')).status === 404, 'a deleted file is not streamed')

  // --- the master without the module ---------------------------------------------------------------------------------------------------
  await master.stop()
  master.env = {}
  await master.start()
  const plain = await master.admin()
  check(Object.keys((await plain.get('/api/session')).json.modules).length === 0 && (await plain.get(`/api/m/video/${id}/hls/master.m3u8`)).status === 404, 'without the module the web app does not offer it')
} catch (e) {
  console.error(e)
  process.exitCode = 1
} finally {
  service?.kill()
  await h.cleanup()
}
console.log(`\nall ${passedChecks()} checks passed`)
