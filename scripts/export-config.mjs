// Writes the configuration file (fafnirx.json) from a master that is already running, so that an existing setup
// can move to the file: devices (with their token hashes, so no device has to be enrolled again), mirrors, sources
// and settings are read from the database and written in the file's format. Works on the database of any earlier
// master. Run it from the folder of docker-compose.yml:
//
//   node scripts/export-config.mjs                     # reads the compose service `db`, writes config/fafnirx.json
//   node scripts/export-config.mjs --out other.json    # somewhere else
//   node scripts/export-config.mjs --container NAME    # `docker exec NAME` instead of `docker compose exec db`
//   node scripts/export-config.mjs --force             # replace a file that is already there
//
// Then restart the master: it reads the file at start. Credentials are written in the clear, so keep the file private.
import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const args = process.argv.slice(2)
const option = (name, fallback) => {
  const i = args.indexOf(name)
  return i >= 0 ? args[i + 1] : fallback
}
const out = path.resolve(option('--out', 'config/fafnirx.json'))
const container = option('--container')
const user = option('--user', 'fafnirx')
const database = option('--database', 'fafnirx')

if (fs.existsSync(out) && !args.includes('--force')) {
  console.error(`${out} already exists; use --force to replace it`)
  process.exit(1)
}

// Whole rows, so that this works whatever columns the database has.
const sql = `select json_build_object(
  'settings', (select coalesce(json_object_agg(key, value), '{}'::json) from settings),
  'devices',  (select coalesce(json_agg(to_jsonb(d) order by name), '[]'::json) from devices d where kind in ('device', 'local')),
  'mirrors',  (select coalesce(json_agg(to_jsonb(m) order by name), '[]'::json) from mirrors m),
  'sources',  (select coalesce(json_agg(to_jsonb(s) order by name), '[]'::json) from sources s))`

const psql = ['psql', '-U', user, '-d', database, '-At', '-c', sql]
const [cmd, cmdArgs] = container
  ? ['docker', ['exec', container, ...psql]]
  : ['docker', ['compose', 'exec', '-T', 'db', ...psql]]
let raw
try {
  raw = execFileSync(cmd, cmdArgs, { encoding: 'utf8', maxBuffer: 64 << 20 })
} catch (e) {
  console.error(`could not read the database: ${e.stderr || e.message}`)
  process.exit(1)
}
const db = JSON.parse(raw)

const json = (text, fallback) => {
  try {
    return JSON.parse(text)
  } catch {
    return fallback
  }
}
const number = (v, fallback) => (v === undefined || v === null || Number.isNaN(Number(v)) ? fallback : Number(v))
const hours = (secs) => Number(secs) / 3600

const config = { version: 1, settings: {}, devices: [], mirrors: [], sources: [] }
for (const key of ['version_count', 'retention_days', 'offline_after_secs']) {
  if (db.settings[key] !== undefined) config.settings[key] = number(db.settings[key])
}

for (const d of db.devices) {
  if (d.kind === 'local') config.devices.push({ name: d.name, kind: 'local' })
  else config.devices.push({ name: d.name, token_hash: d.token_hash, ...(d.revoked_at ? { revoked: true } : {}) })
}

for (const m of db.mirrors) {
  const entry = { name: m.name }
  if (m.remote) entry.remote = m.remote
  else {
    entry.remote_type = m.remote_type
    if (m.remote_path) entry.remote_path = m.remote_path
    const options = json(m.options, {})
    if (Object.keys(options).length) entry.options = options
  }
  entry.interval_hours = hours(m.interval_secs)
  entry.keep_runs = number(m.keep_runs, 3)
  if (!number(m.enabled, 1)) entry.enabled = false
  config.mirrors.push(entry)
}

for (const s of db.sources) {
  const c = json(s.config, {})
  config.sources.push({
    name: s.name,
    kind: s.kind,
    folders: c.folders ?? [],
    service_account: c.service_account ?? {},
    interval_hours: hours(s.interval_secs),
    ...(number(s.enabled, 1) ? {} : { enabled: false }),
  })
}

fs.mkdirSync(path.dirname(out), { recursive: true })
fs.writeFileSync(out, JSON.stringify(config, null, 2) + '\n', { mode: 0o600 })
console.log(
  `wrote ${out}: ${config.devices.length} devices, ${config.mirrors.length} mirrors, ${config.sources.length} sources`,
)
console.log('restart the master to apply it: docker compose up -d master')
