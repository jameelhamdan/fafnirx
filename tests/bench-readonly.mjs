// A read-only benchmark of a running master: only GET requests are sent, so nothing on the server changes.
//
//   node bench-readonly.mjs http://files.local
//
// Without credentials it measures what needs no login: the health check, the session call, the first load of the web app
// and how the server holds up under concurrent requests. With FAFNIRX_USER and FAFNIRX_PASSWORD it also logs in (the one
// POST: it makes a session, like opening the web app does) and measures the library: the timeline, a month, a folder listing,
// thumbnails, and the download speed of a part of an original. It never uploads, deletes, edits or restores anything.
//
// The numbers include the network between this computer and the server. Run it from the machine you use the web app from.

const base = (process.argv[2] ?? 'http://files.local').replace(/\/$/, '')
const user = process.env.FAFNIRX_USER
const password = process.env.FAFNIRX_PASSWORD
const MiB = 1024 * 1024

const pct = (sorted, p) => sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))]
const ms = (n) => `${n.toFixed(1)} ms`
const rows = []
const report = (name, value) => {
  rows.push([name, value])
  console.log(`${name.padEnd(52)} ${value}`)
}

let cookie = ''
const get = (path, headers = {}) => fetch(base + path, { headers: { ...(cookie ? { cookie } : {}), ...headers } })

/** `n` requests one after the other: the latency a person would feel. */
async function sequential(path, n) {
  const times = []
  for (let i = 0; i < n; i++) {
    const t = performance.now()
    const r = await get(path)
    await r.arrayBuffer()
    times.push(performance.now() - t)
  }
  times.sort((a, b) => a - b)
  return { p50: pct(times, 50), p95: pct(times, 95), p99: pct(times, 99), max: times[times.length - 1] }
}

/** `total` requests with `c` of them in flight at once. */
async function concurrent(path, c, total) {
  const times = []
  let next = 0
  let failed = 0
  const started = performance.now()
  await Promise.all(
    Array.from({ length: c }, async () => {
      while (next < total) {
        next++
        const t = performance.now()
        try {
          const r = await get(path)
          await r.arrayBuffer()
          if (!r.ok) failed++
        } catch {
          failed++
        }
        times.push(performance.now() - t)
      }
    }),
  )
  const seconds = (performance.now() - started) / 1000
  times.sort((a, b) => a - b)
  return { rps: total / seconds, p50: pct(times, 50), p99: pct(times, 99), failed }
}

async function timed(path, headers) {
  const t = performance.now()
  const r = await get(path, headers)
  const bytes = (await r.arrayBuffer()).byteLength
  return { ms: performance.now() - t, bytes, status: r.status, r }
}

console.log(`benchmark of ${base} (GET only)\n`)

// --- no login -----------------------------------------------------------------------------------------------------------
await sequential('/healthz', 20) // warm up the connection
for (const [path, label] of [['/healthz', 'health check'], ['/api/session', 'session call (no login)']]) {
  const s = await sequential(path, 300)
  report(`${label}, one at a time`, `p50 ${ms(s.p50)}  p95 ${ms(s.p95)}  p99 ${ms(s.p99)}`)
}
for (const c of [1, 8, 32, 128]) {
  const r = await concurrent('/healthz', c, 2000)
  report(`health check, ${String(c).padStart(3)} at once`, `${r.rps.toFixed(0)} req/s  p50 ${ms(r.p50)}  p99 ${ms(r.p99)}  failed ${r.failed}`)
}

// The first load of the web app: the page and everything it asks for at once.
const page = await timed('/')
const html = new TextDecoder().decode(await (await get('/')).arrayBuffer())
const assets = [...html.matchAll(/(?:src|href)="(\/assets\/[^"]+)"/g)].map((m) => m[1])
const t0 = performance.now()
const parts = await Promise.all(assets.map((a) => timed(a, { 'accept-encoding': 'gzip, br' })))
const load = performance.now() - t0
const plain = await Promise.all(assets.map((a) => timed(a, { 'accept-encoding': 'identity' })))
report('first load of the web app (page + scripts)', `${ms(page.ms + load)}, ${assets.length + 1} files`)
report('  transferred (compressed if the server does)', `${((page.bytes + parts.reduce((n, p) => n + p.bytes, 0)) / 1024).toFixed(0)} KiB`)
report('  uncompressed size', `${((page.bytes + plain.reduce((n, p) => n + p.bytes, 0)) / 1024).toFixed(0)} KiB`)
report('  content-encoding of the main script', parts[0]?.r.headers.get('content-encoding') ?? 'none')
const again = await timed(assets[0], { 'if-none-match': parts[0]?.r.headers.get('etag') ?? '' })
report('  a cached script revalidated', `${ms(again.ms)} (${again.status})`)

// --- with a login ---------------------------------------------------------------------------------------------------------
if (user && password) {
  console.log('')
  const login = await fetch(base + '/api/login', {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'x-requested-with': 'bench' },
    body: JSON.stringify({ username: user, password }),
  })
  if (!login.ok) throw new Error(`login failed: ${login.status}`)
  cookie = login.headers.get('set-cookie').split(';')[0]
  const json = async (path) => (await get(path)).json()

  const s = await sequential('/api/dashboard', 30)
  report('dashboard, one at a time', `p50 ${ms(s.p50)}  p95 ${ms(s.p95)}`)
  const buckets = await json('/api/timeline/buckets')
  report('library size', `${buckets.total.toLocaleString()} photos and videos in ${buckets.months.length} months`)
  const tb = await sequential('/api/timeline/buckets', 30)
  report('timeline skeleton (months and days)', `p50 ${ms(tb.p50)}  p95 ${ms(tb.p95)}`)
  const biggest = [...buckets.months].sort((a, b) => b.count - a.count)[0]
  if (biggest) {
    const tm = await sequential(`/api/timeline?month=${biggest.month}`, 20)
    report(`largest month (${biggest.month}, ${biggest.count} files)`, `p50 ${ms(tm.p50)}  p95 ${ms(tm.p95)}`)
    const assetsOfMonth = (await json(`/api/timeline?month=${biggest.month}`)).assets
    const sample = assetsOfMonth.slice(0, 30)
    if (sample.length) {
      const thumbs = []
      for (const a of sample) {
        const t = await timed(`/api/media/${a.id}/thumb?v=${a.hash}`)
        if (t.status === 200) thumbs.push(t.ms)
      }
      thumbs.sort((a, b) => a - b)
      if (thumbs.length) report(`thumbnails (${thumbs.length} first loads of the month)`, `p50 ${ms(pct(thumbs, 50))}  p95 ${ms(pct(thumbs, 95))}`)
      const again2 = []
      for (const a of sample.slice(0, 20)) again2.push((await timed(`/api/media/${a.id}/thumb?v=${a.hash}`)).ms)
      again2.sort((a, b) => a - b)
      report('  the same thumbnails again', `p50 ${ms(pct(again2, 50))}  p95 ${ms(pct(again2, 95))}`)
      const c = await concurrent(`/api/media/${sample[0].id}/thumb?v=${sample[0].hash}`, 16, 400)
      report('  one thumbnail, 16 at once', `${c.rps.toFixed(0)} req/s  p99 ${ms(c.p99)}`)
      // A part of an original: a range request, so at most 64 MiB cross the network and nothing is changed.
      const big = assetsOfMonth.filter((a) => a.size > 8 * MiB).sort((a, b) => b.size - a.size)[0]
      if (big) {
        const want = Math.min(64 * MiB, big.size)
        const t = performance.now()
        const r = await get(`/api/media/${big.id}/original`, { range: `bytes=0-${want - 1}` })
        const got = (await r.arrayBuffer()).byteLength
        const secs = (performance.now() - t) / 1000
        report(`download of ${(got / MiB).toFixed(0)} MiB of a ${(big.size / MiB).toFixed(0)} MiB original`, `${(got / MiB / secs).toFixed(0)} MiB/s (${r.status})`)
      }
    }
  }
  const devices = await json('/api/devices')
  const device = devices.find((d) => d.files > 0)
  if (device) {
    const f = await sequential(`/api/files?device=${encodeURIComponent(device.folder)}`, 20)
    report(`file list of ${device.folder} (${device.files.toLocaleString()} files)`, `p50 ${ms(f.p50)}  p95 ${ms(f.p95)}`)
    const q = await sequential(`/api/files?device=${encodeURIComponent(device.folder)}&q=a`, 10)
    report('  a search in it', `p50 ${ms(q.p50)}  p95 ${ms(q.p95)}`)
  }
  await fetch(base + '/api/logout', { method: 'POST', headers: { cookie, 'x-requested-with': 'bench' } }).catch(() => {})
}
console.log('\n' + JSON.stringify(Object.fromEntries(rows), null, 2))
