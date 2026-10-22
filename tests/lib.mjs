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
    // A worker serves no HTTP: wait for it to take the leader lock instead.
    if (this.env.FAFNIRX_ROLE === 'worker') {
      await sleep(1500)
      return
    }
    const up = await until(async () => {
      try {
        await fetch(`${this.base}/api/session`) // any answer will do: a process without the api role says 404
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

  /** A logged-in admin API client. */
  async admin() {
    const res = await fetch(`${this.base}/api/login`, {
      method: 'POST',
      headers: { 'content-type': 'application/json', 'x-requested-with': 'e2e' },
      body: JSON.stringify({ password: 'pw' }),
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
      get: (u) => call('GET', u),
      post: (u, b = {}) => call('POST', u, b),
      put: (u, b) => call('PUT', u, b),
      delete: (u) => call('DELETE', u),
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
