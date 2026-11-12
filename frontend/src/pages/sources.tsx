import { useQuery } from '@tanstack/react-query'
import { useApiMutation } from '@/lib/mutation'
import { Pencil, Plus } from 'lucide-react'
import { useState } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { FolderPlace } from '@/components/folder-place'
import { JobList } from '@/components/job-list'
import { StorageForm } from '@/components/storage-form'
import { Button } from '@/components/ui/button'
import { RadioCard, RadioGroup } from '@/components/ui/radio-group'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { FormDialog } from '@/components/form-dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api'
import { NO_STORAGE, storageBody, storageProblem } from '@/lib/storage'
import { docs, filesLink } from '@/lib/links'
import { ScheduleField } from '@/components/schedule-field'
import { bytes, count } from '@/lib/format'
import type { JobState } from '@/lib/types'
import { useT } from '@/lib/i18n'
import { useSourceKinds } from '@/sdk/registry'

/** A source change shows in the sources list and in the files it writes. */
const SOURCE_KEYS = [['sources'], ['files']]

function AddSource() {
  const t = useT()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [services, setServices] = useState(false)
  const [choice, setChoice] = useState('')
  const [storage, setStorage] = useState(NO_STORAGE)
  const [values, setValues] = useState<Record<string, unknown>>({})
  const [hours, setHours] = useState('24')
  const [place, setPlace] = useState('')
  const kinds = useSourceKinds()
  const current = services ? kinds.find((k) => k.kind.id === choice)?.kind : undefined
  const value = current ? (values[current.id] ?? current.initial) : undefined
  const pick = (next: boolean) => { setServices(next); setChoice(next ? (kinds[0]?.kind.id ?? '') : ''); setStorage(NO_STORAGE) }
  const problem = current ? current.problem(value, t) : storageProblem(storage, t)
  const blocked = /^[a-z0-9][a-z0-9_-]*$/i.test(name) ? problem : t('Give it a name of letters, digits, - or _.')
  const body = () => {
    const interval_hours = Number(hours)
    const folder = place ? { folder: place } : undefined
    if (current) return { name, kind: current.id, config: current.body(value), interval_hours, folder }
    return { name, kind: 'rclone', ...storageBody(storage), interval_hours, folder }
  }
  const create = useApiMutation(() => api.post('/api/sources', body()), {
    inline: true,
    invalidate: SOURCE_KEYS,
    onSuccess: () => {
      toast.success(t('Source {name} added. The first run starts within a few seconds.', { name }))
      setOpen(false)
      setName('')
      pick(false)
      setValues({})
      setPlace('')
    },
  })
  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { setOpen(o); create.reset() }}
      trigger={<Button><Plus className="size-4" />{t('Add source')}</Button>}
      className="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
      title={t('Add a source')}
      description={
        <>
          {current ? t(current.intro) : t('The master reads a folder of this storage on a schedule and keeps its own copy. Nothing is ever written back.')}{' '}
          <a className="text-primary hover:underline" href={docs('sources')} target="_blank" rel="noreferrer">{t('Read the guide')} →</a>
        </>
      }
      onSubmit={() => create.mutate()}
      submit={create.isPending ? t('Adding…') : t('Add source')}
      disabled={blocked !== null}
      pending={create.isPending}
      error={create.error?.message}
      footer={<p className={blocked ? 'text-xs text-muted-foreground' : 'text-xs text-success'}>{blocked ?? t('Ready to add.')}</p>}
    >
      <div className="space-y-1.5">
        <Label htmlFor="ss-name">{t('Name')}</Label>
        <Input id="ss-name" placeholder="nextcloud" value={name} onChange={(e) => setName(e.target.value)} />
      </div>
      {kinds.length > 0 && (
        <Tabs value={services ? 'services' : 'storage'} onValueChange={(v) => pick(v === 'services')}>
          <TabsList>
            <TabsTrigger value="storage">{t('Storage')}</TabsTrigger>
            <TabsTrigger value="services">{t('Services')}</TabsTrigger>
          </TabsList>
        </Tabs>
      )}
      {services && (
        <RadioGroup aria-label={t('Services')} className="sm:grid-cols-3" value={choice} onValueChange={setChoice}>
          {kinds.map(({ kind, ready }) => (
            <RadioCard key={kind.id} value={kind.id} disabled={!ready}>
              {t(kind.label)}
              {!ready && <span className="block text-xs text-muted-foreground">{t('Needs setting up')}</span>}
            </RadioCard>
          ))}
        </RadioGroup>
      )}
      {current ? (
        <current.form value={value} onChange={(next) => setValues((all) => ({ ...all, [current.id]: next }))} />
      ) : (
        <StorageForm value={storage} onChange={setStorage} />
      )}
      <FolderPlace own value={place} onChange={setPlace} />
      <ScheduleField id="ss-hours" hours={hours} onChange={setHours} />
    </FormDialog>
  )
}

/** Changes a source: its name, where it writes, how often it runs and, where the kind allows it, what it reads. */
function EditSource({ job }: { job: JobState }) {
  const t = useT()
  const kinds = useSourceKinds()
  const detail = job.detail as { kind: string; folder: string; config: Record<string, unknown> }
  const kind = kinds.find((k) => k.kind.id === detail.kind)?.kind
  const [open, setOpen] = useState(false)
  const [name, setName] = useState(job.name)
  const [place, setPlace] = useState(detail.folder)
  const [hours, setHours] = useState(String(job.interval_secs / 3600))
  const [remotePath, setRemotePath] = useState(String(detail.config?.remote_path ?? ''))
  const [edited, setEdited] = useState<unknown>(undefined)
  const form = kind?.load ? (edited ?? kind.load(detail.config)) : undefined
  const reset = () => {
    setName(job.name)
    setPlace(detail.folder)
    setHours(String(job.interval_secs / 3600))
    setRemotePath(String(detail.config?.remote_path ?? ''))
    setEdited(undefined)
  }
  const rclone = detail.kind === 'rclone' && !detail.config?.remote
  const problem = kind?.load ? kind.problem(form, t) : null
  const save = useApiMutation(() => {
      const body: Record<string, unknown> = { interval_hours: Number(hours) }
      if (name !== job.name) body.name = name
      if (place && place !== detail.folder) body.folder = { folder: place }
      if (kind?.load) body.config = kind.body(form)
      else if (rclone && remotePath !== String(detail.config?.remote_path ?? '')) body.remote_path = remotePath
      return api.patch(`/api/sources/${job.key}`, body)
    }, {
    inline: true,
    invalidate: SOURCE_KEYS,
    onSuccess: () => {
      toast.success(t('Source saved. Changes to where it writes apply from its next run.'))
      setOpen(false)
    },
  })
  const valid = /^[a-z0-9][a-z0-9_-]*$/i.test(name) && place !== null && !problem
  return (
    <FormDialog
      open={open}
      onOpenChange={(o) => { if (o) reset(); setOpen(o); save.reset() }}
      trigger={<Button variant="outline" size="icon" className="size-8" aria-label={t('Edit')}><Pencil className="size-4" /></Button>}
      className="max-h-[90vh] overflow-y-auto sm:max-w-2xl"
      title={t('Edit {name}', { name: job.key })}
      description={t('Passwords and cookies are never shown. Leave them empty to keep what is stored.')}
      onSubmit={() => save.mutate()}
      submit={save.isPending ? t('Saving…') : t('Save')}
      disabled={!valid}
      pending={save.isPending}
      error={save.error?.message}
      footer={<p className="text-xs text-muted-foreground">{problem}</p>}
    >
      <div className="space-y-1.5">
        <Label htmlFor="es-name">{t('Name')}</Label>
        <Input id="es-name" value={name} onChange={(e) => setName(e.target.value)} />
      </div>
      <FolderPlace value={place} onChange={setPlace} />
      {kind?.load ? (
        <kind.form value={form} onChange={setEdited} />
      ) : rclone ? (
        <div className="space-y-1.5">
          <Label htmlFor="es-path">{t('Path')}</Label>
          <Input id="es-path" value={remotePath} onChange={(e) => setRemotePath(e.target.value)} />
        </div>
      ) : null}
      <ScheduleField id="es-hours" hours={hours} onChange={setHours} />
    </FormDialog>
  )
}

export default function Sources() {
  const t = useT()
  const { data, error } = useQuery({ queryKey: ['sources'], queryFn: () => api.get<JobState[]>('/api/sources') })
  return (
    <div>
      <PageHeader
        title={t('Sources')}
        description={t('Things the master collects by itself, on a schedule. Files go into the folder you choose, your own, a shared one or the master files, and appear under Files like any other.')}
        actions={<AddSource />}
      />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-40" /> : (
        <JobList
          kind="sources"
          jobs={data}
          emptyTitle={t('No sources yet')}
          emptyText={t('Add a storage, a Google Drive account or another service to copy their files here regularly.')}
          extraActions={(j) => <EditSource key={j.key} job={j} />}
          summary={(j) => (
            <>
              {String(j.detail.remote)} ·{' '}
              <Link className="underline" to={filesLink(String(j.detail.folder), '')}>{t('{files} files, {size}', { files: count(Number(j.detail.files)), size: bytes(Number(j.detail.bytes)) })}</Link>
            </>
          )}
        />
      )}
    </div>
  )
}
