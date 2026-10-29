import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { BookOpen, Check, Copy, Download, FolderOpen, KeyRound, Lock, Plus, RefreshCw, RotateCcw } from 'lucide-react'
import { useState, type FormEvent, type ReactNode } from 'react'
import { toast } from 'sonner'
import { ErrorNote, PageHeader, SELECT } from '@/components/common'
import { Confirm } from '@/components/confirm'
import { JobList } from '@/components/job-list'
import { ScopePicker } from '@/components/scope-picker'
import { StorageForm } from '@/components/storage-form'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api'
import { NO_STORAGE, storageBody, storageProblem } from '@/lib/storage'
import { bytes, count } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { docs } from '@/lib/links'
import { SCHEDULES } from '@/lib/mirror-presets'
import type { JobState } from '@/lib/types'
import { cn } from '@/lib/utils'

type Kind = 'backup' | 'replica'

const BLANK = {
  kind: 'backup' as Kind,
  name: '',
  storage: NO_STORAGE,
  scope: '',
  interval_hours: '24',
  keep_runs: '3',
  crypt_password: '',
  crypt_salt: '',
  saved: false,
}

/** A random passphrase from the browser's own generator: 192 bits as 32 URL-safe characters. */
function randomSecret() {
  const raw = crypto.getRandomValues(new Uint8Array(24))
  return btoa(String.fromCharCode(...raw)).replaceAll('+', '-').replaceAll('/', '_')
}

function saveText(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'text/plain' }))
  const a = Object.assign(document.createElement('a'), { href: url, download: name })
  a.click()
  URL.revokeObjectURL(url)
}

/** A big, labelled radio button. */
function Choice({ selected, onSelect, icon, title, children }: { selected: boolean; onSelect: () => void; icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={selected}
      onClick={onSelect}
      className={cn('flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition-colors hover:bg-accent/50 focus-visible:outline-2 focus-visible:outline-ring', selected && 'border-primary bg-primary/5 ring-1 ring-primary')}
    >
      <span className="flex items-center gap-2 font-medium">{icon}{title}{selected && <Check className="ml-auto size-4 text-primary" />}</span>
      <span className="text-xs text-muted-foreground">{children}</span>
    </button>
  )
}

function Step({ n, title, children }: { n: number; title: string; children: ReactNode }) {
  return (
    <section className="space-y-3">
      <h3 className="flex items-center gap-2 text-sm font-semibold">
        <span className="grid size-5 place-content-center rounded-full bg-primary text-xs text-primary-foreground">{n}</span>
        {title}
      </h3>
      {children}
    </section>
  )
}

function KeyField({ form, set }: { form: typeof BLANK; set: (patch: Partial<typeof BLANK>) => void }) {
  const t = useT()
  const filled = form.crypt_password !== '' && form.crypt_salt !== ''
  const text = `${t('fafnirx backup key for {name}', { name: form.name || '…' })}\n\n${t('Password')}: ${form.crypt_password}\n${t('Salt')}: ${form.crypt_salt}\n`
  return (
    <div className="space-y-3 rounded-md border border-amber-500/40 bg-amber-500/5 p-3">
      <p className="text-sm">{t('The backup is encrypted with a key only you hold. Without it nothing can be restored, not even by the master.')}</p>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="outline" size="sm" onClick={() => set({ crypt_password: randomSecret(), crypt_salt: randomSecret(), saved: false })}>
          <RefreshCw className="size-4" />{filled ? t('Generate a new key') : t('Generate a key')}
        </Button>
        {filled && (
          <>
            <Button type="button" variant="outline" size="sm" onClick={() => { void navigator.clipboard.writeText(text); toast.success(t('Key copied')) }}><Copy className="size-4" />{t('Copy')}</Button>
            <Button type="button" variant="outline" size="sm" onClick={() => saveText(`${form.name || 'fafnirx'}-backup-key.txt`, text)}><Download className="size-4" />{t('Save as file')}</Button>
          </>
        )}
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1.5">
          <Label htmlFor="m-pw">{t('Password')}</Label>
          <Input id="m-pw" className="font-mono text-xs" autoComplete="off" value={form.crypt_password} onChange={(e) => set({ crypt_password: e.target.value, saved: false })} />
        </div>
        <div className="space-y-1.5">
          <Label htmlFor="m-salt">{t('Salt')}</Label>
          <Input id="m-salt" className="font-mono text-xs" autoComplete="off" value={form.crypt_salt} onChange={(e) => set({ crypt_salt: e.target.value, saved: false })} />
        </div>
      </div>
      <p className="text-xs text-muted-foreground">{t('At least 12 characters each, and different. Your own are fine; a generated key is stronger.')}</p>
      <label className={cn('flex items-center gap-2 text-sm', !filled && 'opacity-50')}>
        <Checkbox checked={form.saved} disabled={!filled} onCheckedChange={(v) => set({ saved: v === true })} />
        {t('I have saved this key somewhere safe, away from the master')}
      </label>
    </div>
  )
}

/** Why the form cannot be sent yet, or null. */
function problem(form: typeof BLANK, t: ReturnType<typeof useT>): string | null {
  if (!/^[a-z0-9][a-z0-9_-]*$/i.test(form.name)) return t('Give it a name of letters, digits, - or _.')
  const storage = storageProblem(form.storage, t)
  if (storage) return storage
  if (form.kind === 'backup') {
    if (form.crypt_password.length < 12 || form.crypt_salt.length < 12 || form.crypt_password === form.crypt_salt) return t('Set the encryption key: two different passwords of at least 12 characters.')
    if (!form.saved) return t('Confirm that you saved the key.')
  }
  return null
}

function AddMirror() {
  const t = useT()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState(BLANK)
  const set = (patch: Partial<typeof BLANK>) => setForm((f) => ({ ...f, ...patch }))
  const blocked = problem(form, t)
  const create = useMutation({
    mutationFn: () =>
      api.post('/api/mirrors', {
        name: form.name,
        kind: form.kind,
        ...storageBody(form.storage),
        scope: form.scope,
        interval_hours: Number(form.interval_hours),
        keep_runs: Number(form.keep_runs),
        crypt_password: form.kind === 'backup' ? form.crypt_password : '',
        crypt_salt: form.kind === 'backup' ? form.crypt_salt : '',
      }),
    onSuccess: () => {
      toast.success(t('Mirror {name} added. The first run starts within a few seconds.', { name: form.name }))
      setOpen(false)
      setForm(BLANK)
      void client.invalidateQueries({ queryKey: ['mirrors'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); if (!blocked) create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />{t('Add mirror')}</Button></DialogTrigger>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <form onSubmit={submit} className="space-y-6">
          <DialogHeader>
            <DialogTitle>{t('Add a mirror')}</DialogTitle>
            <DialogDescription>{t('A mirror copies your files to storage somewhere else, then keeps it up to date.')}</DialogDescription>
          </DialogHeader>

          <Step n={1} title={t('What kind of copy?')}>
            <div role="radiogroup" aria-label={t('Kind of mirror')} className="grid gap-2 sm:grid-cols-2">
              <Choice selected={form.kind === 'backup'} onSelect={() => set({ kind: 'backup' })} icon={<Lock className="size-4" />} title={t('Backup')}>
                {t('Compressed and encrypted with your key. Smallest and safe on storage you do not control. Restored with rclone.')}
              </Choice>
              <Choice selected={form.kind === 'replica'} onSelect={() => set({ kind: 'replica' })} icon={<FolderOpen className="size-4" />} title={t('Replica')}>
                {t('Your files exactly as they are. Browse them, share them or copy them back with any tool.')}
              </Choice>
            </div>
          </Step>

          <Step n={2} title={t('What to copy')}>
            <ScopePicker value={form.scope} onChange={(scope) => set({ scope })} />
          </Step>

          <Step n={3} title={t('Where to put it')}>
            <div className="space-y-1.5">
              <Label htmlFor="m-name">{t('Name')}</Label>
              <Input id="m-name" placeholder={form.kind === 'backup' ? 'offsite-backup' : 'nas-replica'} value={form.name} onChange={(e) => set({ name: e.target.value })} />
            </div>
            <StorageForm value={form.storage} onChange={(storage) => set({ storage })} />
          </Step>

          {form.kind === 'backup' && (
            <Step n={4} title={t('Encryption key')}>
              <KeyField form={form} set={set} />
            </Step>
          )}

          <Step n={form.kind === 'backup' ? 5 : 4} title={t('Schedule')}>
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="m-hours">{t('How often')}</Label>
                <select id="m-hours" className={cn(SELECT, 'w-full')} value={form.interval_hours} onChange={(e) => set({ interval_hours: e.target.value })}>
                  {SCHEDULES.map(([hours, label]) => <option key={hours} value={hours}>{t(label)}</option>)}
                </select>
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="m-keep">{t('Runs of history kept')}</Label>
                <Input id="m-keep" type="number" min="1" max="100" value={form.keep_runs} onChange={(e) => set({ keep_runs: e.target.value })} />
              </div>
            </div>
            <p className="text-xs text-muted-foreground">{t('Only what changed is sent. Files you replace or delete stay in the history for the runs kept, then go.')}</p>
          </Step>

          {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          <DialogFooter className="items-center gap-3 sm:justify-between">
            <p className={cn('text-xs', blocked ? 'text-muted-foreground' : 'text-success')}>{blocked ?? t('Ready to add.')}</p>
            <Button type="submit" disabled={create.isPending || blocked !== null}>{create.isPending ? t('Adding…') : t('Add mirror')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function Guide({ open }: { open: boolean }) {
  const t = useT()
  return (
    <details open={open} className="mb-6 rounded-lg border bg-card p-4 text-sm">
      <summary className="flex cursor-pointer items-center gap-2 font-medium"><BookOpen className="size-4" />{t('How mirrors work')}</summary>
      <div className="mt-3 grid gap-4 sm:grid-cols-2">
        <div className="space-y-1">
          <p className="flex items-center gap-2 font-medium"><Lock className="size-4" />{t('Backup')}</p>
          <p className="text-muted-foreground">{t('Names and contents are encrypted with your key and compressed before they leave the master. Use it for cloud storage. To restore, download the restore kit and run rclone.')}</p>
        </div>
        <div className="space-y-1">
          <p className="flex items-center gap-2 font-medium"><FolderOpen className="size-4" />{t('Replica')}</p>
          <p className="text-muted-foreground">{t('The same folders and files as the master, readable by anything. Use it for a NAS or a disk you want to browse.')}</p>
        </div>
      </div>
      <ul className="mt-3 list-disc space-y-1 pl-5 text-muted-foreground">
        <li>{t('Only new and changed files are sent; a renamed file is moved on the mirror, not sent again.')}</li>
        <li>{t('A mirror never deletes: replaced and deleted files go to a history of runs, and only the oldest runs are removed.')}</li>
        <li>{t('Keep the key of a backup away from the master: a disk that fails takes the key with it.')}</li>
      </ul>
      <a className="mt-3 inline-block text-primary hover:underline" href={docs('mirrors')} target="_blank" rel="noreferrer">{t('Read the full guide')} →</a>
    </details>
  )
}

export default function Mirrors() {
  const t = useT()
  const client = useQueryClient()
  const [resync, setResync] = useState<string | null>(null)
  const { data, error } = useQuery({ queryKey: ['mirrors'], queryFn: () => api.get<JobState[]>('/api/mirrors') })
  const resyncNow = useMutation({
    mutationFn: (name: string) => api.post(`/api/mirrors/${name}/resync`),
    onSuccess: (_d, name) => { toast.success(t('{name} will re-send every file on its next run.', { name })); void client.invalidateQueries({ queryKey: ['mirrors'] }) },
    onError: (e) => toast.error(e.message),
  })
  const kit = useMutation({
    mutationFn: (name: string) => api.get<{ config: string }>(`/api/mirrors/${name}/restore-kit`).then((r) => ({ name, config: r.config })),
    onSuccess: ({ name, config }) => saveText(`${name}-restore.conf`, config),
    onError: (e) => toast.error(e.message),
  })
  return (
    <div>
      <PageHeader
        title={t('Mirrors')}
        description={t('Copies of your files on other storage. Without one, the master is the only copy.')}
        actions={<AddMirror />}
      />
      <Guide open={data?.length === 0} />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-40" /> : (
        <JobList
          kind="mirrors"
          jobs={data}
          emptyTitle={t('No mirrors yet')}
          emptyText={t('Add one so that a failed disk does not lose your files.')}
          summary={(j) => {
            const backup = j.detail.kind === 'backup'
            return (
              <div className="space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <Badge variant={backup ? 'default' : 'secondary'}>{backup ? <><Lock className="size-3" />{t('Backup')}</> : <><FolderOpen className="size-3" />{t('Replica')}</>}</Badge>
                  <span>{j.detail.scope ? String(j.detail.scope) : t('Everything')} → {j.detail.remote ? String(j.detail.remote) : `${String(j.detail.remote_type)}:${String(j.detail.remote_path || '')}`}</span>
                </div>
                <div>
                  {t('keeps {count} runs', { count: String(j.detail.keep_runs) })}
                  {j.detail.files !== null && <> · {t('last run {files} files, {size}', { files: count(Number(j.detail.files)), size: bytes(Number(j.detail.bytes)) })}</>}
                </div>
              </div>
            )
          }}
          extraActions={(j) => (
            <>
              {j.detail.kind === 'backup' && (
                <Button variant="outline" size="icon" className="size-8" aria-label={t('Download restore kit')} title={t('Download restore kit')} disabled={kit.isPending} onClick={() => kit.mutate(j.key)}><KeyRound className="size-4" /></Button>
              )}
              <Button variant="outline" size="icon" className="size-8" aria-label={t('Re-send everything')} title={t('Re-send everything')} onClick={() => setResync(j.name)}><RotateCcw className="size-4" /></Button>
            </>
          )}
        />
      )}
      <Confirm
        open={resync !== null}
        title={t('Re-send everything to {name}?', { name: resync ?? '' })}
        description={t('Use this after emptying the remote. The next run uploads every file again.')}
        action={t('Re-send')}
        onCancel={() => setResync(null)}
        onConfirm={() => { if (resync) resyncNow.mutate(resync); setResync(null) }}
      />
    </div>
  )
}
