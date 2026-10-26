// End-to-end test of the delete strategy.
//
// The client is the authority: a file deleted on the device is only marked deleted on the master. Its data
// stays recoverable for the retention period, then the file, its versions and their stored copies are removed.
// The master's cleanup job first runs 30 s after start, so this test takes about two minutes.
//   cd tests && npm install && node e2e-retention.mjs
import { randomBytes } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { check, enrollDevice, harness, passedChecks, sleep } from './lib.mjs'

const DAY = 86400
const h = await harness('fafnirx-e2e-ret-')
const { tmp, data, schema, master } = h

/** Where the stored copy of every version of a path is. A deleted file keeps all of them under versions/. */
const versionFiles = async (p) =>
  (await schema.rows('SELECT v.id, d.folder FROM versions v JOIN files f ON f.id = v.file_id JOIN devices d ON d.id = f.device_id WHERE f.path = $1', [p])).map(
    ([id, device]) => path.join(data, 'versions', String(id), device, p),
  )
const deletedAt = async (p) => {
  const rows = await schema.rows('SELECT deleted_at FROM files WHERE path = $1', [`docs/${p}`])
  return rows.length === 0 ? undefined : rows[0][0]
}

try {
  await master.start()
  const admin = await master.admin()
  const { c } = await enrollDevice(h, admin)
  const src = path.join(tmp, 'docs')
  fs.mkdirSync(src)
  c('add-folder', src)

  const original = randomBytes(5000)
  fs.writeFileSync(path.join(src, 'gone.txt'), original)
  fs.writeFileSync(path.join(src, 'keep.txt'), randomBytes(5000))
  fs.writeFileSync(path.join(src, 'shared.txt'), 'identical content in two files')
  fs.writeFileSync(path.join(src, 'shared-copy.txt'), 'identical content in two files')
  c('sync')
  check((await deletedAt('gone.txt')) === null, 'file collected and live')

  // --- the client deletes: the master only marks it ---------------------------------------------------------
  fs.unlinkSync(path.join(src, 'gone.txt'))
  c('sync')
  check((await deletedAt('gone.txt')) != null, 'deleting on the client marks the file deleted on the master')
  const first = await versionFiles('docs/gone.txt')
  check(first.length > 0 && first.every(fs.existsSync) && fs.readFileSync(first[0]).equals(original), 'its file is still stored (moved to versions/) and intact')

  // --- the client wins again if the file comes back -----------------------------------------------------------
  fs.writeFileSync(path.join(src, 'gone.txt'), 'recreated on the client')
  c('sync')
  check((await deletedAt('gone.txt')) === null, 'recreating the file on the client revives it')
  check((await versionFiles('docs/gone.txt')).length === 2, 'the old content is kept as an earlier version')
  fs.unlinkSync(path.join(src, 'gone.txt'))
  fs.unlinkSync(path.join(src, 'shared-copy.txt'))
  c('sync')
  check((await deletedAt('gone.txt')) != null && (await deletedAt('shared-copy.txt')) != null, 'both deletions are marked')

  const expired = [...(await versionFiles('docs/gone.txt')), ...(await versionFiles('docs/shared-copy.txt'))]
  const keep = path.join(data, 'files', 'users', 'admin', 'laptop', 'docs', 'keep.txt')
  check(expired.every(fs.existsSync) && fs.existsSync(keep), 'stored copies are on disk')

  const restartAndWaitForCleanup = async () => {
    await master.stop()
    await master.start()
    await sleep(36_000) // the cleanup job runs 30 s after start
  }
  const backdate = (days) => schema.rows('UPDATE files SET deleted_at = $1 WHERE deleted_at IS NOT NULL', [Math.floor(Date.now() / 1000) - days * DAY])

  // --- inside the retention period nothing is removed ------------------------------------------------------------
  await backdate(29)
  await restartAndWaitForCleanup()
  check((await deletedAt('gone.txt')) != null && expired.every(fs.existsSync), '29 days after the delete the data is still there')

  // --- after the retention period it is removed for good ------------------------------------------------------------
  await backdate(31)
  await restartAndWaitForCleanup()
  check((await deletedAt('gone.txt')) === undefined && (await deletedAt('shared-copy.txt')) === undefined, '31 days after the delete the file records are gone')
  const left = (await schema.rows("SELECT COUNT(*) FROM versions v JOIN files f ON f.id = v.file_id WHERE f.path IN ('docs/gone.txt', 'docs/shared-copy.txt')"))[0][0]
  check(Number(left) === 0, 'and so are all their versions')
  check(!expired.some(fs.existsSync), 'the stored copies of expired files are deleted from disk')
  check(fs.readFileSync(path.join(data, 'files', 'users', 'admin', 'laptop', 'docs', 'shared.txt')).toString() === 'identical content in two files', 'an identical file that was never deleted is untouched')
  check((await deletedAt('keep.txt')) === null && fs.readFileSync(keep).equals(fs.readFileSync(path.join(src, 'keep.txt'))), 'files that were never deleted are untouched, byte for byte')

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  await h.cleanup()
}
