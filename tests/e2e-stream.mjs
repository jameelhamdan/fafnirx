// End-to-end test of adaptive video streaming: the real master in front of the real fafnirx-stream service.
//
// ffmpeg is not needed: small stand-in programs answer for ffprobe and ffmpeg, so that what is tested is everything
// around them: the web login in front, the token between the two services, the playlists, which stretch of the
// video each segment asks ffmpeg for, that a segment is made once and kept, and that the service refuses paths that
// leave the file tree. (Real transcoding needs ffmpeg and is checked by hand: docs/streaming.md.)
//   cd tests && npm install && node e2e-stream.mjs
import { spawn } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { BIN, check, ENV, EXE, freePort, harness, MASTER, passedChecks, until } from './lib.mjs'

const STREAM = path.join(BIN, `fafnirx-stream${EXE}`)
const TOKEN = 'stream-token-for-the-test-0123456789'
const h = await harness('fafnirx-e2e-stream-', { binaries: [MASTER, STREAM] })
const { tmp, data, schema, master } = h

// --- stand-ins for ffprobe and ffmpeg -------------------------------------------------------------------------------
const fakes = path.join(tmp, 'fakes')
fs.mkdirSync(fakes)
const calls = path.join(tmp, 'ffmpeg-calls.log')
fs.writeFileSync(
  path.join(fakes, 'ffprobe.mjs'),
  `console.log(JSON.stringify({ streams: [{ codec_type: 'video', width: 1280, height: 720 }, { codec_type: 'audio' }], format: { duration: '14.5' } }))\n`,
)
fs.writeFileSync(
  path.join(fakes, 'ffmpeg.mjs'),
  `import fs from 'node:fs'
const a = process.argv.slice(2)
const at = (flag) => a[a.indexOf(flag) + 1]
fs.appendFileSync(${JSON.stringify(calls)}, 'ss=' + at('-ss') + ' t=' + at('-t') + ' vf=' + at('-vf') + '\\n')
fs.writeFileSync(a[a.length - 1], 'TS ss=' + at('-ss') + ' t=' + at('-t'))\n`,
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

const streamPort = await freePort()
const streamUrl = `http://127.0.0.1:${streamPort}`
let service = null
const calledSegments = () => (fs.existsSync(calls) ? fs.readFileSync(calls, 'utf8').trim().split('\n').filter(Boolean) : [])

try {
  const log = fs.openSync(path.join(tmp, 'stream.log'), 'a')
  service = spawn(STREAM, [], {
    stdio: ['ignore', log, log],
    env: {
      ...ENV,
      FAFNIRX_STREAM_TOKEN: TOKEN,
      FAFNIRX_DATA: data,
      FAFNIRX_STREAM_CACHE: path.join(tmp, 'stream-cache'),
      FAFNIRX_STREAM_LISTEN: `127.0.0.1:${streamPort}`,
      FAFNIRX_FFMPEG: program('ffmpeg'),
      FAFNIRX_FFPROBE: program('ffprobe'),
    },
  })
  check(await until(async () => { try { return (await fetch(`${streamUrl}/health`)).ok } catch { return false } }, 20, 100), 'the streaming service starts')

  master.env = { FAFNIRX_STREAM_URL: streamUrl, FAFNIRX_STREAM_TOKEN: TOKEN }
  await master.start()
  const admin = await master.admin()
  check((await admin.get('/api/session')).json.stream === true, 'the web app is told that streaming is on')

  // A video in a local folder.
  await admin.post('/api/devices', { name: 'videos', kind: 'local' })
  fs.mkdirSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip'), { recursive: true })
  fs.writeFileSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip', 'clip.mkv'), 'pretend this is a video')
  const id = await until(async () => (await schema.rows("SELECT id FROM files WHERE path = 'trip/clip.mkv'"))[0]?.[0], 30, 300)
  check(id, 'the master finds the video')

  // --- the playlists ------------------------------------------------------------------------------------------------------
  const stream = (rest) => admin.get(`/api/stream/${id}/${rest}`)
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
  const anonymous = await fetch(`${master.base}/api/stream/${id}/master.m3u8`)
  check(anonymous.status === 401, 'the master asks for a login first')
  const direct = (p, headers = {}) => fetch(`${streamUrl}/hls/master.m3u8?p=${p}&h=abc123`, { headers })
  check((await direct('admin/videos/trip/clip.mkv')).status === 401, 'the service itself refuses a request without the token')
  check((await direct('admin/videos/trip/clip.mkv', { authorization: 'Bearer wrong-token-wrong-token' })).status === 401, 'and with a wrong one')
  const ok = { authorization: `Bearer ${TOKEN}` }
  check((await direct('admin/videos/trip/clip.mkv', ok)).status === 200, 'and answers with the right one')
  check((await direct('videos/..%2F..%2Fsecret', ok)).status === 400 && (await direct('..%2Fx%2Fy', ok)).status === 400, 'the service refuses paths that leave the file tree')

  // --- a file that is gone or changed ---------------------------------------------------------------------------------------------
  fs.writeFileSync(path.join(data, 'files', 'users', 'admin', 'videos', 'trip', 'clip.mkv'), 'a different video, another content hash')
  const changed = await until(async () => (await schema.rows("SELECT encode(v.root_hash, 'hex') FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path = 'trip/clip.mkv' ORDER BY v.id")).length >= 2, 30, 300)
  check(changed, 'the master records the new content')
  const before = calledSegments().length
  check((await stream('720p/seg1.ts')).status === 200 && calledSegments().length === before + 1, 'changed content gets new segments, never stale ones')

  await admin.post('/api/files/delete', { items: [{ device: 'admin/videos', path: 'trip/clip.mkv' }] })
  check((await stream('master.m3u8')).status === 404, 'a deleted file is not streamed')

  // --- the master without a service ---------------------------------------------------------------------------------------------------
  await master.stop()
  master.env = {}
  await master.start()
  const plain = await master.admin()
  check((await plain.get('/api/session')).json.stream === false && (await plain.get(`/api/stream/${id}/master.m3u8`)).status === 404, 'without a streaming service the web app does not offer it')
} catch (e) {
  console.error(e)
  process.exitCode = 1
} finally {
  service?.kill()
  await h.cleanup()
}
console.log(`\nall ${passedChecks()} checks passed`)
