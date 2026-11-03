import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { JobList } from '@/components/job-list'
import { StorageForm } from '@/components/storage-form'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api'
import { NO_SERVICE, NO_STORAGE, storageBody, storageProblem } from '@/lib/storage'
import { docs } from '@/lib/links'
import { SCHEDULES } from '@/lib/mirror-presets'
import { SELECT } from '@/components/common'
import { bytes, count } from '@/lib/format'
import type { JobState } from '@/lib/types'
import { useT } from '@/lib/i18n'
import { useSourceKinds } from '@/sdk/registry'
import { cn } from '@/lib/utils'

const CARD = 'rounded-md border px-3 py-2 text-left text-sm transition-colors hover:bg-accent/50'
const CARD_ON = 'border-primary bg-primary/5 ring-1 ring-primary'

/** The service that is part of the app: Google Drive, read with rclone. The others come from modules (`sdk`, `SourceKind`). */
const DRIVE = 'drive'

function AddSource() {
  const t = useT()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [services, setServices] = useState(false)
  const [choice, setChoice] = useState(DRIVE)
  const [storage, setStorage] = useState(NO_STORAGE)
  const [values, setValues] = useState<Record<string, unknown>>({})
  const [hours, setHours] = useState('24')
  const kinds = useSourceKinds()
  const current = services ? kinds.find((k) => k.kind.id === choice)?.kind : undefined
  const value = current ? (values[current.id] ?? current.initial) : undefined
  const pick = (next: boolean) => { setServices(next); setChoice(DRIVE); setStorage(next ? NO_SERVICE : NO_STORAGE) }
  const problem = current ? current.problem(value, t) : storageProblem(storage, t)
  const blocked = /^[a-z0-9][a-z0-9_-]*$/i.test(name) ? problem : t('Give it a name of letters, digits, - or _.')
  const body = () => {
    const interval_hours = Number(hours)
    if (current) return { name, kind: current.id, config: current.body(value), interval_hours }
    return { name, kind: 'rclone', ...storageBody(storage), interval_hours }
  }
  const create = useMutation({
    mutationFn: () => api.post('/api/sources', body()),
    onSuccess: () => {
      toast.success(t('Source {name} added. The first run starts within a few seconds.', { name }))
      setOpen(false)
      setName('')
      pick(false)
      setValues({})
      void client.invalidateQueries({ queryKey: ['sources'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); if (!blocked) create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />{t('Add source')}</Button></DialogTrigger>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <form onSubmit={submit} className="space-y-5">
          <DialogHeader>
            <DialogTitle>{t('Add a source')}</DialogTitle>
            <DialogDescription>
              {current ? t(current.intro) : t('The master reads a folder of this storage on a schedule and keeps its own copy. Nothing is ever written back.')}{' '}
              <a className="text-primary hover:underline" href={docs('sources')} target="_blank" rel="noreferrer">{t('Read the guide')} →</a>
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <Label htmlFor="ss-name">{t('Name')}</Label>
            <Input id="ss-name" placeholder="nextcloud" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div role="tablist" className="flex gap-1 border-b">
            {[[false, t('Storage')], [true, t('Services')]].map(([s, label]) => (
              <button
                key={String(s)}
                type="button"
                role="tab"
                aria-selected={services === s}
                onClick={() => pick(s as boolean)}
                className={`-mb-px border-b-2 px-3 py-1.5 text-sm ${services === s ? 'border-primary font-medium' : 'border-transparent text-muted-foreground hover:text-foreground'}`}
              >
                {label as string}
              </button>
            ))}
          </div>
          {services && (
            <div role="radiogroup" aria-label={t('Services')} className="grid gap-2 sm:grid-cols-3">
              <button type="button" role="radio" aria-checked={choice === DRIVE} onClick={() => setChoice(DRIVE)} className={cn(CARD, choice === DRIVE && CARD_ON)}>
                {t('Google Drive')}
              </button>
              {kinds.map(({ kind, ready }) => (
                <button key={kind.id} type="button" role="radio" aria-checked={choice === kind.id} disabled={!ready} onClick={() => setChoice(kind.id)} className={cn(CARD, choice === kind.id && CARD_ON, 'disabled:cursor-not-allowed disabled:opacity-50')}>
                  {t(kind.label)}
                  {!ready && <span className="block text-xs text-muted-foreground">{t('Needs setting up')}</span>}
                </button>
              ))}
            </div>
          )}
          {current ? (
            <current.form value={value} onChange={(next) => setValues((all) => ({ ...all, [current.id]: next }))} />
          ) : (
            <StorageForm services={services} value={storage} onChange={setStorage} />
          )}
          <div className="space-y-1.5">
            <Label htmlFor="ss-hours">{t('How often')}</Label>
            <select id="ss-hours" className={`${SELECT} w-full`} value={hours} onChange={(e) => setHours(e.target.value)}>
              {SCHEDULES.map(([h, label]) => <option key={h} value={h}>{t(label)}</option>)}
            </select>
          </div>
          {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          <DialogFooter className="items-center gap-3 sm:justify-between">
            <p className={blocked ? 'text-xs text-muted-foreground' : 'text-xs text-success'}>{blocked ?? t('Ready to add.')}</p>
            <Button type="submit" disabled={create.isPending || blocked !== null}>{create.isPending ? t('Adding…') : t('Add source')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Sources() {
  const t = useT()
  const { data, error } = useQuery({ queryKey: ['sources'], queryFn: () => api.get<JobState[]>('/api/sources') })
  return (
    <div>
      <PageHeader
        title={t('Sources')}
        description={t("Things the master collects by itself, on a schedule. Files land in their own folder and appear under Files like any device's.")}
        actions={<AddSource />}
      />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-40" /> : (
        <JobList
          kind="sources"
          jobs={data}
          emptyTitle={t('No sources yet')}
          emptyText={t('Add a storage, a Google Drive account or another service to copy their files here regularly.')}
          summary={(j) => (
            <>
              {String(j.detail.remote)} ·{' '}
              <Link className="underline" to={`/files?device=${String(j.detail.device)}`}>{t('{files} files, {size}', { files: count(Number(j.detail.files)), size: bytes(Number(j.detail.bytes)) })}</Link>
            </>
          )}
        />
      )}
    </div>
  )
}
