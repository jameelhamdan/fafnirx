// End-to-end test of shared folders: the real master, the real web API and several real client binaries, as different computers
// of one user and of another.
//   cd tests && npm install && node e2e-shared.mjs
import fs from 'node:fs'
import path from 'node:path'
import { CLIENT, MASTER, check, enrollDevice, harness, namesIn as names, passedChecks, readIn as read, sleep, startDaemon, stopDaemon, until, writeIn as write } from './lib.mjs'

const h = await harness('fafnirx-shared-', { binaries: [MASTER, CLIENT] })
const { tmp, data, schema, master } = h

/** What the master holds for a path of a folder, or null: the live file, or null when it is deleted or not there. */
const stored = (folder, p) => {
  const file = path.join(data, 'files', 'users', folder, p)
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8') : null
}
const live = async (folder) =>
  (await schema.rows('SELECT f.path FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = $1 AND f.deleted_at IS NULL ORDER BY f.path', [folder])).map((r) => r[0])

try {
  await master.start()
  const admin = await master.admin()
  for (const [name, role] of [['alice', 'normal'], ['bob', 'normal']]) {
    check((await admin.post('/api/users', { name, role, password: 'password-1' })).status === 200, `user ${name} added`)
  }
  const api = { alice: await master.admin('alice', 'password-1'), bob: await master.admin('bob', 'password-1') }

  /** A computer of a user: a device with its own client configuration and state. */
  const computer = (owner, name) =>
    enrollDevice(h, api[owner], name, `client-${owner}-${name}`, undefined, `state-${owner}-${name}`)
  const laptop = await computer('alice', 'laptop')
  const desktop = await computer('alice', 'desktop')
  const phone = await computer('bob', 'phone')
  const dirL = path.join(tmp, 'laptop-family')
  const dirD = path.join(tmp, 'desktop-family')
  const dirB = path.join(tmp, 'bob-family')

  // --- make one and attach the first folder -----------------------------------------------------------------------------------------
  write(dirL, 'a.txt', 'hello')
  write(dirL, 'docs/b.txt', 'second')
  const attached = laptop.c('shared', 'attach', 'family', dirL, '--create').stdout
  check(attached.includes('"alice/family"'), `a shared folder is made and attached from the command line (${attached.trim()})`)
  check(laptop.c('shared', 'list').stdout.trim() === 'alice/family\town', 'the list names it as <owner>/<name> and marks the user\'s own')
  check(laptop.c('folders').stdout.includes('(shared, both ways)'), 'and `folders` shows it')
  const first = laptop.c('sync').stdout
  check(first.includes('2 uploaded'), `the first sync uploads its files (${first.trim()})`)
  check(stored('alice/family', 'docs/b.txt') === 'second', 'they are in the folder of the shared folder on the master, at its root')
  check((await live('alice/laptop')).length === 0, 'and are not also backed up to the computer\'s own device')

  // --- a second computer of the same user ---------------------------------------------------------------------------------------------
  desktop.c('shared', 'attach', 'alice/family', dirD)
  const received = desktop.c('sync').stdout
  check(received.includes('2 downloaded') && read(dirD, 'a.txt') === 'hello' && read(dirD, 'docs/b.txt') === 'second', `another computer receives them (${received.trim()})`)
  check(desktop.c('sync').stdout.includes('0 uploaded') && !desktop.c('sync').stdout.includes('downloaded'), 'and does not send them back')

  // --- a change on one computer reaches the other ----------------------------------------------------------------------------------------
  write(dirD, 'a.txt', 'hello from the desktop')
  check(desktop.c('sync').stdout.includes('1 uploaded'), 'an edit is sent')
  const pulled = laptop.c('sync').stdout
  check(pulled.includes('1 downloaded') && read(dirL, 'a.txt') === 'hello from the desktop', `and received by the other computer (${pulled.trim()})`)

  // --- both change the same file: nothing is lost -------------------------------------------------------------------------------------------
  write(dirL, 'a.txt', 'L')
  write(dirD, 'a.txt', 'D')
  desktop.c('sync')
  const conflict = laptop.c('sync').stdout
  const kept = names(dirL).filter((n) => n.includes('(conflict '))
  check(read(dirL, 'a.txt') === 'D' && kept.length === 1 && read(dirL, kept[0]) === 'L', `a file changed on both sides keeps both (${kept})`)
  check(conflict.includes('1 conflicts') && stored('alice/family', kept[0]) === 'L', 'the copy of this computer is sent like a new file')
  desktop.c('sync')
  check(read(dirD, kept[0]) === 'L', 'and reaches the other computer')

  // --- delete and rename ---------------------------------------------------------------------------------------------------------------------
  fs.rmSync(path.join(dirD, 'docs', 'b.txt'))
  desktop.c('sync')
  laptop.c('sync')
  check(read(dirL, 'docs/b.txt') === null, 'a file deleted on one computer is deleted on the other')
  check((await live('alice/family')).every((p) => p !== 'docs/b.txt'), 'and is in the trash on the master')
  write(dirL, 'c.txt', 'to be renamed, with some content')
  laptop.c('sync')
  desktop.c('sync')
  fs.renameSync(path.join(dirL, 'c.txt'), path.join(dirL, 'd.txt'))
  const renamed = laptop.c('sync').stdout
  check(renamed.includes('1 renamed') && renamed.includes('0 bytes sent'), `a rename is sent as a rename (${renamed.trim()})`)
  desktop.c('sync')
  check(read(dirD, 'd.txt') === 'to be renamed, with some content' && read(dirD, 'c.txt') === null, 'and arrives as a rename')

  // --- the same names on every kind of computer ------------------------------------------------------------------------------------------------
  write(dirL, 'Notes.txt', 'one')
  laptop.c('sync')
  check((await api.alice.upload('alice/family', 'NOTES.TXT', 'two')) === 409, 'the master refuses a path that differs only by case from one in the folder')
  check((await api.alice.upload('alice/family', 'what?.txt', 'x')) === 400, 'and a name Windows cannot keep')
  check((await api.alice.upload('alice/family', 'from-the-web.txt', 'web')) === 200, 'while an ordinary upload through the web app is a file like any other')
  laptop.c('sync')
  check(read(dirL, 'from-the-web.txt') === 'web', 'which the computers receive')

  // --- renaming to another case, which the master used to refuse --------------------------------------------------------------------------------
  write(dirL, 'Case.txt', 'only the case changes')
  laptop.c('sync')
  desktop.c('sync')
  fs.renameSync(path.join(dirL, 'Case.txt'), path.join(dirL, 'case.txt'))
  const caseRenamed = laptop.c('sync').stdout
  check(caseRenamed.includes('1 renamed') && caseRenamed.includes('0 failed') && caseRenamed.includes('0 bytes sent'), `a file renamed to another case is a rename (${caseRenamed.trim()})`)
  check((await live('alice/family')).includes('case.txt') && !(await live('alice/family')).includes('Case.txt'), 'and the master has the new spelling only')
  desktop.c('sync')
  check(names(dirD).includes('case.txt') && !names(dirD).includes('Case.txt'), 'the other computer gets it under the new spelling, not left under the old')
  write(dirL, 'Trip/a.txt', 'first')
  write(dirL, 'Trip/b.txt', 'second')
  laptop.c('sync')
  desktop.c('sync')
  fs.renameSync(path.join(dirL, 'Trip'), path.join(dirL, 'trip'))
  const folderRenamed = laptop.c('sync').stdout
  check(folderRenamed.includes('2 renamed') && folderRenamed.includes('0 failed'), `a folder renamed to another case moves its files (${folderRenamed.trim()})`)
  const family = await live('alice/family')
  check(family.includes('trip/a.txt') && family.includes('trip/b.txt') && !family.some((p) => p.startsWith('Trip/')), 'the master has one spelling of the folder')
  desktop.c('sync')
  check(names(dirD).includes('trip') && !names(dirD).includes('Trip') && read(dirD, 'trip/b.txt') === 'second', 'and so does the other computer')
  check((await api.alice.upload('alice/family', 'TRIP/new.txt', 'x')) === 409, 'a new file under another spelling of that folder is refused')
  check((await api.alice.upload('alice/family', 'trip/new.txt', 'x')) === 200, 'under the same spelling it is not')
  // --- a computer away longer than the master remembers deletions -----------------------------------------------------------------------------------
  write(dirL, 'old.txt', 'will be deleted while the laptop is away')
  laptop.c('sync')
  desktop.c('sync')
  check(read(dirD, 'old.txt') !== null, 'both computers have a file')
  fs.rmSync(path.join(dirD, 'old.txt'))
  desktop.c('sync')
  // Retention has since removed the record of the deletion, and the master says from when it knows them all again.
  await schema.rows("DELETE FROM versions WHERE file_id IN (SELECT f.id FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = 'alice/family' AND f.path = 'old.txt')")
  await schema.rows("DELETE FROM files WHERE id IN (SELECT f.id FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = 'alice/family' AND f.path = 'old.txt')")
  await schema.rows("INSERT INTO settings (key, value) VALUES ('deletions_known_from', $1) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value", [String(Math.floor(Date.now() / 1000) + 100)])
  const away = laptop.c('sync').stdout
  check(read(dirL, 'old.txt') === null && away.includes('1 deleted'), `the computer that was away still learns of the deletion (${away.trim()})`)
  check(read(dirL, 'a.txt') === 'D', 'and keeps every file that is still there')

  // --- a large deletion on the master is held until a person decides ------------------------------------------------------------------------------------
  const lot = (dir, n) => Array.from({ length: n }, (_, i) => `${dir}/${i}.txt`)
  const deleteInTheWebApp = (paths) => api.alice.post('/api/files/delete', { items: paths.map((p) => ({ device: 'alice/family', path: p })) })
  // What is deleted in the web app is applied on every computer at once, however many: it is deliberate and in the Trash.
  for (const p of lot('web', 12)) write(dirL, p, `file ${p}`)
  laptop.c('sync')
  desktop.c('sync')
  check(lot('web', 12).every((p) => read(dirD, p) !== null), 'twelve more files are on both computers')
  check((await deleteInTheWebApp(lot('web', 12))).status === 200, 'they are deleted in the web app')
  desktop.c('sync')
  check(lot('web', 12).every((p) => read(dirD, p) === null), 'a computer deletes them at once, they are not held')
  check(!desktop.c('status').stdout.includes('holding'), 'and nothing is waiting')
  laptop.c('sync')
  write(dirL, 'web/0.txt', 'made again')
  laptop.c('sync')
  check(stored('alice/family', 'web/0.txt') === 'made again', 'a file made again at a path deleted in the web app is stored, not taken for deleted')
  desktop.c('sync')
  check(read(dirD, 'web/0.txt') === 'made again', 'and reaches the other computer')

  // What another computer deletes, in large part, is held until a person decides.
  const deleteFromTheLaptop = (paths) => {
    for (const p of paths) fs.rmSync(path.join(dirL, p))
    laptop.c('sync')
    laptop.c('release-deletes')
    laptop.c('sync')
  }
  for (const p of lot('hold', 12)) write(dirL, p, `file ${p}`)
  laptop.c('sync')
  desktop.c('sync')
  check(lot('hold', 12).every((p) => read(dirD, p) !== null), 'twelve more files are on both computers again')
  deleteFromTheLaptop(lot('hold', 12))
  const holder = startDaemon(desktop)
  const heldCount = (text) => Number(/holding (\d+) files/.exec(text)?.[1] ?? 0)
  check(await until(() => heldCount(desktop.c('status').stdout) >= 12, 60), 'a client that sees a large part of the folder deleted holds them back')
  check(lot('hold', 12).every((p) => read(dirD, p) !== null), 'and deletes nothing')
  check(await until(async () => (await api.alice.get('/api/dashboard')).json.alerts.some((a) => a.kind === 'deletions_held' && a.key === 'alice/desktop'), 60), 'the owner gets an alert on the dashboard')
  stopDaemon(holder)
  await sleep(1500)
  const stopped = desktop.c('status').stdout
  check(heldCount(stopped) >= 12 && stopped.includes('deletions held back'), 'with the client stopped, `status` still says so')
  const heldJson = JSON.parse(desktop.c('status', '--json').stdout)
  check(heldJson.shared.find((s) => s.name === 'alice/family')?.held >= 12, 'and so does `status --json`, from the folder\'s own state')
  const released = desktop.c('release-deletes').stdout
  check(released.includes('applied at the next cycle'), `release-deletes reaches the shared folder (${released.split('\n')[0]})`)
  const deleted = desktop.c('sync').stdout
  check(Number(/(\d+) deleted/.exec(deleted)?.[1]) >= 12 && lot('hold', 12).every((p) => read(dirD, p) === null), `and the files are deleted here (${deleted.trim()})`)
  check(!desktop.c('status').stdout.includes('holding'), 'nothing is held after that')

  // --- the master's own folders keep one spelling of a name too, for what a person adds in the web app --------------------------------------------------
  check((await api.alice.post('/api/devices', { name: 'library', kind: 'local' })).status === 200, 'a local folder is made')
  check((await api.alice.upload('alice/library', 'Docs/a.txt', 'x')) === 200, 'a file is added to it')
  check((await api.alice.upload('alice/library', 'docs/b.txt', 'y')) === 409, 'another spelling of its folder is refused')
  check((await api.alice.upload('alice/library', 'what?.txt', 'y')) === 400, 'and so is a name Windows cannot keep')

  // --- a change reaches a running client within seconds, not at its next poll --------------------------------------------------------------------
  const daemon = startDaemon(desktop)
  check(await until(() => desktop.c('status').stdout.split('\n').slice(0, 2).join(' ').includes('online'), 30), 'a client runs in the background')
  await sleep(3000)
  write(dirL, 'pushed.txt', 'sent while the other computer was watching')
  laptop.c('sync')
  const started = Date.now()
  check(await until(() => read(dirD, 'pushed.txt') !== null, 40, 250), 'the change arrives')
  const took = Math.round((Date.now() - started) / 1000)
  check(took < 20, `and within seconds, without waiting for the next poll (${took} s)`)

  // --- what the client says about its shared folders ------------------------------------------------------------------------------------------------
  // The folder holds a path that cannot be received (it differs only by case from another), which stays a problem to look at.
  const said = await until(() => /alice\/family \(.*\): (in step|needs attention)/.test(desktop.c('status').stdout), 40)
  const statusText = desktop.c('status').stdout
  check(said && statusText.includes('differs only by case'), `status lists the shared folder and how it is doing (${statusText.split('\n').find((l) => l.includes('alice/family'))?.trim()})`)
  const asJson = JSON.parse(desktop.c('status', '--json').stdout)
  const entry = asJson.status.shared.find((s) => s.name === 'alice/family')
  check(asJson.running === true && entry?.files > 0 && entry.last_cycle > 0 && entry.last_error.includes('differs only by case'), 'and `status --json` has the same for scripts')
  const onMaster = await until(async () => (await api.alice.get('/api/devices/alice%2Fdesktop')).json?.shared?.some((s) => s.name === 'alice/family'), 60)
  check(onMaster, 'the web app shows it on the computer\'s page')
  const card = (await api.alice.get('/api/shared')).json.find((f) => f.folder === 'alice/family')
  check(card.computers.some((c) => c.name === 'alice/desktop' && c.last_cycle > 0), 'and on the folder, with the computers that have it')
  stopDaemon(daemon)
  await sleep(1500)

  // --- another user: nothing until the owner adds them --------------------------------------------------------------------------------------------
  check(!phone.c('shared', 'list').stdout.includes('alice/family'), 'another user does not see the shared folder')
  check(phone.c.try('shared', 'attach', 'alice/family', dirB).code !== 0, 'and cannot attach it')
  check((await api.bob.get('/api/files?device=alice/family')).status === 404, 'nor see it in the web app')
  check((await api.alice.post('/api/shared/family/members', { user: 'bob', can_write: false })).status === 200, 'the owner adds them to read')
  check(phone.c('shared', 'list').stdout.includes('alice/family'), 'now they see it')
  check((await api.bob.get('/api/files?device=alice/family')).status === 200, 'in the web app too')
  phone.c('shared', 'attach', 'alice/family', dirB)
  phone.c('sync')
  check(read(dirB, 'a.txt') === 'D' && read(dirB, 'd.txt') !== null, 'and receive its files')
  write(dirB, 'bobs.txt', 'not allowed yet')
  const refused = phone.c.try('sync')
  check(`${refused.stdout}${refused.stderr}`.includes('read only'), 'a change from a read-only member is refused, and says why')
  check(stored('alice/family', 'bobs.txt') === null, 'nothing of theirs reached the folder')
  check((await api.alice.post('/api/shared/family/members', { user: 'bob', can_write: true })).status === 200, 'the owner lets them change it')
  check(phone.c('sync').stdout.includes('1 uploaded') && stored('alice/family', 'bobs.txt') === 'not allowed yet', 'and their file goes up')

  // --- the owner takes it away -------------------------------------------------------------------------------------------------------------------------
  check((await api.alice.delete('/api/shared/family/members/bob')).status === 200, 'the owner removes them')
  check(phone.c.try('sync').code !== 0 && !phone.c('shared', 'list').stdout.includes('alice/family'), 'they lose it at once')
  phone.c('shared', 'detach', 'alice/family')
  check(read(dirB, 'a.txt') === 'D', 'detaching keeps the files they have')

  // --- deletions held back can be kept instead (last for the desktop: files deleted in the web app cannot come back from here) ----------------------------
  for (const p of lot('keep', 12)) write(dirL, p, `file ${p}`)
  laptop.c('sync')
  desktop.c('sync')
  deleteFromTheLaptop(lot('keep', 12))
  desktop.c('sync')
  check(heldCount(desktop.c('status').stdout) >= 12, 'twelve more deletions are held')
  const keeping = desktop.c('shared', 'keep', 'family').stdout
  check(/keeping the \d+ files/.test(keeping), `they can be kept instead (${keeping.split('\n')[0]})`)
  desktop.c('sync')
  check(lot('keep', 12).every((p) => read(dirD, p) !== null) && !desktop.c('status').stdout.includes('holding'), 'the files stay and nothing is held any more')
  check(desktop.c('shared', 'keep', 'family').stdout.includes('no deletions of the master are held'), 'keeping when nothing is held says so')

  // --- delete the files, then the folder ------------------------------------------------------------------------------------------------------------------
  check((await api.alice.delete('/api/shared/family')).status === 409, 'a folder with files cannot be deleted')
  const items = (await live('alice/family')).map((p) => ({ device: 'alice/family', path: p }))
  check((await api.alice.post('/api/files/delete', { items })).status === 200, 'its files are deleted')
  check((await api.alice.delete('/api/shared/family')).status === 200, 'then the folder can be')
  check(!fs.existsSync(path.join(data, 'files', 'users', 'alice', 'family')) && (await schema.rows("SELECT 1 FROM devices WHERE folder = 'alice/family'")).length === 0, 'it takes its trash and its place on disk with it')
  const gone = laptop.c.try('sync')
  check(gone.code !== 0 && `${gone.stdout}${gone.stderr}`.includes('no shared folder'), 'a computer that still has it attached is told it is gone')
  laptop.c('shared', 'detach', 'family')
  check(read(dirL, 'a.txt') === 'D', 'and keeps its files when it lets go')

  console.log(`\nall ${passedChecks()} checks passed`)
} finally {
  await h.cleanup()
}
