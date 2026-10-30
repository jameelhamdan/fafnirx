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
import { NO_STORAGE, storageBody, storageProblem } from '@/lib/storage'
import { docs } from '@/lib/links'
import { SCHEDULES } from '@/lib/mirror-presets'
import { SELECT } from '@/components/common'
import { bytes, count } from '@/lib/format'
import type { JobState } from '@/lib/types'
import { useT } from '@/lib/i18n'

function AddSource() {
  const t = useT()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [storage, setStorage] = useState(NO_STORAGE)
  const [hours, setHours] = useState('24')
  const blocked = /^[a-z0-9][a-z0-9_-]*$/i.test(name) ? storageProblem(storage, t) : t('Give it a name of letters, digits, - or _.')
  const create = useMutation({
    mutationFn: () => api.post('/api/sources', { name, ...storageBody(storage), interval_hours: Number(hours) }),
    onSuccess: () => {
      toast.success(t('Source {name} added. The first run starts within a few seconds.', { name }))
      setOpen(false)
      setName('')
      setStorage(NO_STORAGE)
      void client.invalidateQueries({ queryKey: ['sources'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); if (!blocked) create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />{t('Add storage source')}</Button></DialogTrigger>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <form onSubmit={submit} className="space-y-5">
          <DialogHeader>
            <DialogTitle>{t('Copy files in from other storage')}</DialogTitle>
            <DialogDescription>
              {t('The master reads a folder of this storage on a schedule and keeps its own copy. Nothing is ever written back.')}{' '}
              <a className="text-primary hover:underline" href={docs('sources')} target="_blank" rel="noreferrer">{t('Read the guide')} →</a>
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-1.5">
            <Label htmlFor="ss-name">{t('Name')}</Label>
            <Input id="ss-name" placeholder="nextcloud" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <StorageForm source value={storage} onChange={setStorage} />
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
          emptyText={t('Add a Google Drive account or other storage to copy its folders here regularly.')}
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
