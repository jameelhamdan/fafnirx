// Helpers shared by the end-to-end scripts.
//
// Needs a PostgreSQL server named by FAFNIRX_TEST_DATABASE_URL, for example
//   docker run -d -e POSTGRES_PASSWORD=test -p 55432:5432 postgres:17
//   FAFNIRX_TEST_DATABASE_URL=postgresql://postgres:test@localhost:55432/postgres
// and the binaries from `cargo build --workspace` (set FAFNIRX_BIN_DIR when the target directory is elsewhere).
// Every run gets its own schema, so runs never see each other.

import { spawn, spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import pg from 'pg'

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
export const EXE = process.platform === 'win32' ? '.exe' : ''
export const BIN = process.env.FAFNIRX_BIN_DIR ?? path.join(ROOT, 'target', 'debug')
export const MASTER = path.join(BIN, `fafnirx-master${EXE}`)
export const CLIENT = path.join(BIN, `fafnirx${EXE}`)
export const SIGN = path.join(BIN, `fafnirx-sign${EXE}`)
export const CHUNK = 4 * 1024 * 1024
export const ENV = { ...process.env, FAFNIRX_TOKEN_STORE: 'file', RUST_LOG: 'info' }

let passed = 0
export const passedChecks = () => passed

export function check(cond, msg) {
  if (!cond) {
    console.error(`FAIL: ${msg}`)
    process.exit(1)
  }
  passed++
  console.log(`  ok  ${msg}`)
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/** Writes a file below dir, making the folders. */
export const writeIn = (dir, p, text) => {
  fs.mkdirSync(path.dirname(path.join(dir, p)), { recursive: true })
  fs.writeFileSync(path.join(dir, p), text)
}
/** The text of a file below dir, or null when it is not there. */
export const readIn = (dir, p) => (fs.existsSync(path.join(dir, p)) ? fs.readFileSync(path.join(dir, p), 'utf8') : null)
/** The names in a folder, sorted. */
export const namesIn = (dir) => fs.readdirSync(dir).sort()

const daemons = new Set()
/** Runs the client of client in the background; it is stopped by stopDaemon or at the end of the run. */
export function startDaemon(client) {
  const daemon = spawn(CLIENT, ['--config', client.cfg, 'run'], { env: ENV, stdio: 'ignore' })
  daemons.add(daemon)
  return daemon
}
export function stopDaemon(daemon) {
  daemon.kill()
  daemons.delete(daemon)
}

/** Polls `fn` until it returns something truthy, for at most `seconds`. */
export async function until(fn, seconds = 30, every = 250) {
  const end = Date.now() + seconds * 1000
  for (;;) {
    const v = await fn()
    if (v) return v
    if (Date.now() > end) return v
    await sleep(every)
  }
}

export function freePort() {
  return new Promise((resolve) => {
    const s = net.createServer().listen(0, '127.0.0.1', () => {
      const { port } = s.address()
      s.close(() => resolve(port))
    })
  })
}

/** Runs a command to the end; exits the whole run with its output when it fails and `ok` is true. */
export function run(exe, args, { ok = true, timeout = 120_000, cwd, input } = {}) {
  const r = spawnSync(exe, args.map(String), { encoding: 'utf8', env: ENV, timeout, cwd, input })
  if (ok && r.status !== 0) {
    console.error(`command failed: ${exe} ${args.join(' ')}\n${r.stdout}\n${r.stderr}`)
    process.exit(1)
  }
  return { code: r.status, stdout: r.stdout ?? '', stderr: r.stderr ?? '' }
}

export function mkdtemp(prefix = 'fafnirx-e2e-') {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix))
}

/** A schema of its own in the test database; returns its connection URL and a way to drop it. */
export async function createSchema() {
  const base = process.env.FAFNIRX_TEST_DATABASE_URL
  if (!base) {
    console.error('set FAFNIRX_TEST_DATABASE_URL (postgresql://user:password@host:port/database)')
    process.exit(1)
  }
  const schema = `e2e_${randomBytes(6).toString('hex')}`
  const admin = new pg.Client({ connectionString: base })
  await admin.connect()
  await admin.query(`CREATE SCHEMA ${schema}`)
  const url = `${base}${base.includes('?') ? '&' : '?'}options=${encodeURIComponent(`-c search_path=${schema}`)}`
  const db = new pg.Client({ connectionString: url })
  await db.connect()
  return {
    url,
    /** Runs a query and returns the rows as arrays of values. */
    rows: async (sql, params = []) => (await db.query({ text: sql, values: params, rowMode: 'array' })).rows,
    drop: async () => {
      await db.end()
      await admin.query(`DROP SCHEMA ${schema} CASCADE`)
      await admin.end()
    },
  }
}

/** The master binary, started and stopped by the test. */
export class Master {
  constructor({ data, dbUrl, http, grpc, env = {} }) {
    Object.assign(this, { data, dbUrl, http, grpc, env, proc: null })
    this.base = `http://127.0.0.1:${http}`
  }

  async start() {
    const log = fs.openSync(path.join(path.dirname(this.data), 'master.log'), 'a')
    this.proc = spawn(MASTER, [], {
      stdio: ['ignore', log, log],
      env: {
        ...ENV,
        FAFNIRX_DATABASE_URL: this.dbUrl,
        FAFNIRX_DATA: this.data,
        FAFNIRX_LISTEN: `127.0.0.1:${this.grpc}`,
        FAFNIRX_HTTP: `127.0.0.1:${this.http}`,
        FAFNIRX_ADMIN_PASSWORD: 'pw',
        FAFNIRX_TICK_SECS: '1',
        ...this.env,
      },
    })
    const up = await until(async () => {
      try {
        await fetch(`${this.base}/api/session`) // any answer will do
        return true
      } catch {
        return false
      }
    }, 30, 100)
    if (!up) {
      console.error('master did not start; see master.log next to the data directory')
      process.exit(1)
    }
  }

  async stop() {
    if (!this.proc) return
    this.proc.kill()
    await new Promise((r) => (this.proc.exitCode !== null ? r() : this.proc.once('exit', r)))
    this.proc = null
  }

  /** A logged-in API client: the administrator by default, or another user. */
  async admin(username = 'admin', password = 'pw') {
    const res = await fetch(`${this.base}/api/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-requested-with': 'e2e' },
      body: JSON.stringify({ username, password }),
    })
    if (!res.ok) throw new Error(`login failed: ${res.status}`)
    const cookie = res.headers.get('set-cookie').split(';')[0]
    const call = async (method, url, body) => {
      const r = await fetch(this.base + url, {
        method,
        headers: { cookie, 'x-requested-with': 'e2e', ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
        body: body === undefined ? undefined : JSON.stringify(body),
      })
      const text = await r.text()
      let json = null
      try {
        json = JSON.parse(text)
      } catch {
        /* not JSON (a file) */
      }
      return { status: r.status, json, text }
    }
    return {
      cookie,
      get: (u) => call('GET', u),
      post: (u, b = {}) => call('POST', u, b),
      put: (u, b) => call('PUT', u, b),
      delete: (u) => call('DELETE', u),
      /** Uploads a file through the web API (start, bytes, finish); returns the status of finishing it. */
      upload: async (device, p, text) => {
        const body = Buffer.from(text)
        const started = (await call('POST', '/api/uploads', { device, path: p, size: body.length })).json
        await fetch(`${this.base}/api/uploads/${started.id}?offset=0`, { method: 'PUT', headers: { cookie, 'x-requested-with': 'e2e' }, body })
        return (await call('POST', `/api/uploads/${started.id}/finish`, {})).status
      },
      /** Raw bytes of a download. */
      bytes: async (u, headers = {}) => {
        const r = await fetch(this.base + u, { headers: { cookie, ...headers } })
        return { status: r.status, body: Buffer.from(await r.arrayBuffer()), headers: r.headers }
      },
    }
  }
}

/** A client config directory with the commands to drive the real client binary. */
export function clientIn(dir, server) {
  const cfg = path.join(dir, 'config.toml')
  const c = (...args) => run(CLIENT, ['--config', cfg, ...args])
  c.try = (...args) => run(CLIENT, ['--config', cfg, ...args], { ok: false })
  /** Runs a command that reads the device token from standard input, as `enroll` does. */
  c.enroll = (token, server, { ok = true } = {}) =>
    run(CLIENT, ['--config', cfg, 'enroll', '--no-service', '--server', server], { ok, input: `${token}\n` })
  return { cfg, c, server }
}

/** What every script starts from: the binaries checked, a temp directory with a data directory, a schema and a master. */
export async function harness(prefix = 'fafnirx-e2e-', { binaries = [MASTER, CLIENT], env = {} } = {}) {
  for (const b of binaries) {
    if (!fs.existsSync(b)) {
      console.error(`missing ${b}; run cargo build --workspace first`)
      process.exit(1)
    }
  }
  const tmp = mkdtemp(prefix)
  console.log('working in', tmp)
  const data = path.join(tmp, 'data')
  fs.mkdirSync(data)
  const schema = await createSchema()
  const master = new Master({ data, dbUrl: schema.url, http: await freePort(), grpc: await freePort(), env })
  return {
    tmp,
    data,
    schema,
    master,
    /** Stops the master, drops the schema and removes the temp directory (unless FAFNIRX_KEEP=1). */
    async cleanup() {
      daemons.forEach(stopDaemon)
      await master.stop()
      await schema.drop()
      if (process.env.FAFNIRX_KEEP === '1') console.log('kept', tmp)
      else fs.rmSync(tmp, { recursive: true, force: true })
    },
  }
}

/**
 * Gives the client of a config file a state directory of its own, as a second computer has. By default it also turns self-update
 * off: a client that updates itself would fetch the newest real release and replace the binary under test.
 */
export function isolate(cfg, stateDir, { autoUpdate = false } = {}) {
  const text = fs.readFileSync(cfg, 'utf8')
  fs.writeFileSync(cfg, `state_dir = "${stateDir.replaceAll('\\', '/')}"\n${autoUpdate ? text : text.replace('auto_update = true', 'auto_update = false')}`)
}

/**
 * Creates a device through the web API and enrolls a client for it, keeping the client's state in `state` inside `tmp`.
 * `admin` is the API session that makes it: the administrator (for any user), or the user themself.
 */
export async function enrollDevice({ tmp, master }, admin, name = 'laptop', dir = 'client', user = undefined, state = 'state') {
  const token = (await admin.post('/api/devices', { name, ...(user ? { user } : {}) })).json.token
  const client = clientIn(path.join(tmp, dir))
  client.c.enroll(token, `http://127.0.0.1:${master.grpc}`)
  isolate(client.cfg, path.join(tmp, state))
  // Enrolling ran with the default state directory, which is one computer; the client of the test keeps its state in
  // `tmp`, which is another. Release the device so that this one can have it.
  const folder = encodeURIComponent((await admin.get('/api/devices')).json.find((d) => d.name === name && (!user || d.user === user))?.folder ?? `admin/${name}`)
  await admin.post(`/api/devices/${folder}/release`)
  return { ...client, token }
}
