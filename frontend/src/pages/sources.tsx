import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { Link } from 'react-router-dom'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { JobList } from '@/components/job-list'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { Textarea } from '@/components/ui/textarea'
import { api } from '@/lib/api'
import { bytes, count } from '@/lib/format'
import type { JobState } from '@/lib/types'

function AddSource() {
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState({ name: '', folders: '', service_account: '', interval_hours: '24' })
  const set = (k: keyof typeof form) => (e: { target: { value: string } }) => setForm({ ...form, [k]: e.target.value })
  const create = useMutation({
    mutationFn: () => api.post('/api/sources', { ...form, interval_hours: Number(form.interval_hours) }),
    onSuccess: () => {
      toast.success(`Source ${form.name} added. The first run starts within a few seconds.`)
      setOpen(false)
      setForm({ name: '', folders: '', service_account: '', interval_hours: '24' })
      void client.invalidateQueries({ queryKey: ['sources'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />Add Google Drive source</Button></DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Add a Google Drive source</DialogTitle>
            <DialogDescription>
              In Google Cloud, create a service account, download its JSON key, enable the Drive API, and share each Drive folder with the account's email (viewer is enough).
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="s-name">Name</Label>
            <Input id="s-name" placeholder="photos" value={form.name} onChange={set('name')} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="s-folders">Folders</Label>
            <Textarea id="s-folders" rows={3} placeholder="One folder id or folder URL per line" value={form.folders} onChange={set('folders')} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="s-key">Service account key (JSON)</Label>
            <Textarea id="s-key" rows={4} className="font-mono text-xs" placeholder='{"client_email": "...", "private_key": "..."}' value={form.service_account} onChange={set('service_account')} />
            <p className="text-xs text-muted-foreground">Stored on the master and never shown again.</p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="s-hours">Check every (hours)</Label>
            <Input id="s-hours" type="number" min="0.25" step="any" value={form.interval_hours} onChange={set('interval_hours')} />
          </div>
          {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          <DialogFooter><Button type="submit" disabled={create.isPending}>Add source</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Sources() {
  const { data, error } = useQuery({ queryKey: ['sources'], queryFn: () => api.get<JobState[]>('/api/sources') })
  return (
    <div>
      <PageHeader
        title="Sources"
        description="Things the master collects by itself, on a schedule. Files land in their own folder and appear under Files like any device's."
        actions={<AddSource />}
      />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-40" /> : (
        <JobList
          kind="sources"
          jobs={data}
          emptyTitle="No sources yet"
          emptyText="Add a Google Drive account to copy its folders here regularly."
          summary={(j) => (
            <>
              Google Drive · {String(j.detail.folders)} folder(s) · {String(j.detail.account || 'no account')} ·{' '}
              <Link className="underline" to={`/files?device=${String(j.detail.device)}`}>{count(Number(j.detail.files))} files, {bytes(Number(j.detail.bytes))}</Link>
            </>
          )}
        />
      )}
    </div>
  )
}
