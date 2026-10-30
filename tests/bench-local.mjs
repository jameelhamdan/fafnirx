// A benchmark of the master on this computer: a throwaway master and database, a synthetic photo library, uploads and
// downloads. Nothing outside the temporary folder is touched. Thumbnails are off (they are made on demand, which is not
// measured here), so the numbers are those of recording, reading, uploading and downloading files.
//
//   FAFNIRX_BIN_DIR=target/release FAFNIRX_TEST_DATABASE_URL=postgresql://... node bench-local.mjs [tiny=5000] [large=200]
//   BENCH_CORES=1   pins the master to that many cores (Windows only), to find the least that does the work
//
// Needs Python with Pillow (to make the photos), and the test database in a Docker container named by BENCH_PG (default
// fx-test-pg) to read its memory. Loopback has no network in it, so this measures the server and not the link.
import { execFileSync, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { check, harness, sleep } from './lib.mjs'

const here = path.dirname(fileURLToPath(import.meta.url))
const TINY = Number(process.argv[2] ?? 5000)
const LARGE = Number(process.argv[3] ?? 200)
const PY = process.env.PYTHON ?? 'python'
const PG = process.env.BENCH_PG ?? 'fx-test-pg'
const MiB = 1048576

const h = await harness('fafnirx-bench-', { binaries: [path.join(process.env.FAFNIRX_BIN_DIR, process.platform === 'win32' ? 'fafnirx-master.exe' : 'fafnirx-master')] })
const { master, data } = h
const win = process.platform === 'win32'
const ps = (cmd) => spawnSync('powershell', ['-NoProfile', '-Command', cmd], { encoding: 'utf8' }).stdout.trim()
const rssMiB = (pid) => (win ? Number(ps(`(Get-Process -Id ${pid}).WorkingSet64`)) / MiB : Number(spawnSync('ps', ['-o', 'rss=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim()) / 1024)
const cpuSeconds = (pid) => (win ? Number(ps(`(Get-Process -Id ${pid}).TotalProcessorTime.TotalSeconds`)) : 0)
/** What the database container holds in memory, in MiB. */
const pgMiB = () => {
  const out = spawnSync('docker', ['stats', PG, '--no-stream', '--format', '{{.MemUsage}}'], { encoding: 'utf8' }).stdout.trim()
  const m = /^([\d.]+)(MiB|GiB|KiB)/.exec(out)
  return m ? Number(m[1]) * { KiB: 1 / 1024, MiB: 1, GiB: 1024 }[m[2]] : NaN
}
const fmt = (n) => (Number.isFinite(n) ? n.toFixed(0) : '?')

try {
  await master.start()
  const cores = Number(process.env.BENCH_CORES ?? 0)
  if (cores > 0 && win) {
    ps(`(Get-Process -Id ${master.proc.pid}).ProcessorAffinity = ${(1 << cores) - 1}`)
    console.log(`master pinned to ${cores} core(s)`)
  }
  const admin = await master.admin()
  await admin.put('/api/settings', { version_count: 5, retention_days: 30, thumbnails: false })
  console.log(`idle: master ${fmt(rssMiB(master.proc.pid))} MiB, PostgreSQL ${fmt(pgMiB())} MiB`)

  /** Waits until every file is in the library and the queue is empty; says how long it took and the peaks of memory and CPU. */
  async function drain(label, expected, added) {
    const t0 = performance.now()
    const cpu0 = cpuSeconds(master.proc.pid)
    let peak = 0
    let pgPeak = 0
    let quiet = 0
    for (let i = 0; performance.now() - t0 < 30 * 60_000; i++) {
      peak = Math.max(peak, rssMiB(master.proc.pid))
      if (i % 3 === 0) pgPeak = Math.max(pgPeak, pgMiB() || 0)
      const q = (await admin.get('/api/queue')).json
      const busy = (q?.summary ?? []).reduce((n, s) => n + s.queued + s.running, 0)
      const total = (await admin.get('/api/timeline/buckets')).json?.total ?? 0
      quiet = busy === 0 && total >= expected ? quiet + 1 : 0
      if (quiet >= 3) break
      await sleep(1000)
    }
    const secs = (performance.now() - t0) / 1000
    const cpu = cpuSeconds(master.proc.pid) - cpu0
    console.log(`${label}: ${secs.toFixed(1)} s, ${(added / secs).toFixed(1)} files/s; master peak ${fmt(peak)} MiB, ${cpu.toFixed(1)} CPU-s (${(cpu / secs).toFixed(2)} cores); PostgreSQL peak ${fmt(pgPeak)} MiB`)
  }

  const folder = path.join(data, 'files', 'users', 'admin', 'lib')
  let expected = 0
  if (TINY > 0) {
    execFileSync(PY, [path.join(here, 'bench-library.py'), folder, 'tiny', String(TINY)], { stdio: 'ignore' })
    expected += TINY
    await drain(`record and read ${TINY} small photos`, expected, TINY)
  }
  if (LARGE > 0) {
    const sample = path.join(folder, 'large')
    execFileSync(PY, [path.join(here, 'bench-library.py'), sample, 'large', String(LARGE)], { stdio: 'ignore' })
    console.log(`large photos are 3000 x 2000, ${(fs.statSync(path.join(sample, 'large_0000.jpg')).size / 1e6).toFixed(1)} MB each`)
    expected += LARGE
    await drain(`record and read ${LARGE} large photos`, expected, LARGE)
  }

  // Uploads through the web API, in chunks as the web app does it.
  await admin.post('/api/devices', { name: 'up', kind: 'local' })
  const upload = async (relative, size, chunkSize) => {
    const started = await admin.post('/api/uploads', { device: 'admin/up', path: relative, size })
    const id = started.json.id
    const chunk = Buffer.alloc(Math.min(chunkSize, size), 3)
    for (let offset = 0; offset < size; offset += chunk.length) {
      const body = offset + chunk.length > size ? chunk.subarray(0, size - offset) : chunk
      const r = await fetch(`${master.base}/api/uploads/${id}?offset=${offset}`, { method: 'PUT', headers: { cookie: admin.cookie, 'x-requested-with': 'bench' }, body })
      if (!r.ok) throw new Error(`upload chunk: ${r.status}`)
    }
    const done = await admin.post(`/api/uploads/${id}/finish`, {})
    if (done.status !== 200) throw new Error(`finish: ${done.status} ${done.text}`)
  }
  let t = performance.now()
  await upload('big.bin', 512 * MiB, 32 * MiB)
  console.log(`upload of one 512 MiB file (32 MiB chunks): ${(512 / ((performance.now() - t) / 1000)).toFixed(0)} MiB/s`)
  t = performance.now()
  const small = 300
  const lanes = 4
  await Promise.all(Array.from({ length: lanes }, async (_, lane) => {
    for (let i = lane; i < small; i += lanes) await upload(`small/f${i}.bin`, 64 * 1024, 64 * 1024)
  }))
  console.log(`upload of ${small} files of 64 KiB (${lanes} at once): ${(small / ((performance.now() - t) / 1000)).toFixed(0)} files/s`)

  // Downloads.
  for (const range of [null, 'bytes=0-134217727']) {
    t = performance.now()
    const r = await fetch(`${master.base}/api/files/download?device=admin%2Fup&path=big.bin`, { headers: { cookie: admin.cookie, ...(range ? { range } : {}) } })
    let got = 0
    for await (const part of r.body) got += part.length
    console.log(`download of ${(got / MiB).toFixed(0)} MiB${range ? ' (range)' : ''}: ${(got / MiB / ((performance.now() - t) / 1000)).toFixed(0)} MiB/s`)
  }
  console.log(`after the load: master ${fmt(rssMiB(master.proc.pid))} MiB, PostgreSQL ${fmt(pgMiB())} MiB\n`)
  check(true, 'done')
  const out = spawnSync(process.execPath, [path.join(here, 'bench-readonly.mjs'), master.base], {
    env: { ...process.env, FAFNIRX_USER: 'admin', FAFNIRX_PASSWORD: 'pw' },
    encoding: 'utf8',
  })
  console.log(out.stdout.split('\n{')[0])
  if (out.status !== 0) console.error(out.stderr)
} finally {
  await h.cleanup()
}
