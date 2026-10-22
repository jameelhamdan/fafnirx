import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus, RotateCcw } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { Confirm } from '@/components/confirm'
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

const BLANK = { name: '', remote_type: 's3', options: '', remote_path: '', interval_hours: '72', keep_runs: '3' }

function AddMirror() {
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const [form, setForm] = useState(BLANK)
  const set = (k: keyof typeof form) => (e: { target: { value: string } }) => setForm({ ...form, [k]: e.target.value })
  const create = useMutation({
    mutationFn: () => api.post('/api/mirrors', { ...form, interval_hours: Number(form.interval_hours), keep_runs: Number(form.keep_runs) }),
    onSuccess: () => {
      toast.success(`Mirror ${form.name} added. The first run starts within a few seconds.`)
      setOpen(false)
      setForm(BLANK)
      void client.invalidateQueries({ queryKey: ['mirrors'] })
    },
  })
  const submit = (e: FormEvent) => { e.preventDefault(); create.mutate() }
  return (
    <Dialog open={open} onOpenChange={(o) => { setOpen(o); create.reset() }}>
      <DialogTrigger asChild><Button><Plus className="size-4" />Add mirror</Button></DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <form onSubmit={submit} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Add an offsite mirror</DialogTitle>
            <DialogDescription>The master copies <code>files/</code> and what changed since the last run to any storage rclone supports.</DialogDescription>
          </DialogHeader>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="m-name">Name</Label>
              <Input id="m-name" placeholder="offsite" value={form.name} onChange={set('name')} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="m-type">rclone backend</Label>
              <Input id="m-type" placeholder="s3, sftp, webdav, b2…" value={form.remote_type} onChange={set('remote_type')} />
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="m-path">Path on the remote</Label>
            <Input id="m-path" placeholder="my-bucket/fafnirx" value={form.remote_path} onChange={set('remote_path')} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="m-options">Backend options</Label>
            <Textarea id="m-options" rows={5} className="font-mono text-xs" placeholder={'provider=AWS\naccess_key_id=…\nsecret_access_key=…\nregion=eu-west-1'} value={form.options} onChange={set('options')} />
            <p className="text-xs text-muted-foreground"><code>key=value</code> per line. Stored on the master and never shown again.</p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-2">
              <Label htmlFor="m-hours">Run every (hours)</Label>
              <Input id="m-hours" type="number" min="0.25" step="any" value={form.interval_hours} onChange={set('interval_hours')} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="m-keep">Runs of history kept</Label>
              <Input id="m-keep" type="number" min="1" max="100" value={form.keep_runs} onChange={set('keep_runs')} />
            </div>
          </div>
          {create.error && <p role="alert" className="text-sm text-destructive">{create.error.message}</p>}
          <DialogFooter><Button type="submit" disabled={create.isPending}>Add mirror</Button></DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export default function Mirrors() {
  const client = useQueryClient()
  const [resync, setResync] = useState<string | null>(null)
  const { data, error } = useQuery({ queryKey: ['mirrors'], queryFn: () => api.get<JobState[]>('/api/mirrors') })
  const resyncNow = useMutation({
    mutationFn: (name: string) => api.post(`/api/mirrors/${name}/resync`),
    onSuccess: (_d, name) => { toast.success(`${name} will re-send every file on its next run.`); void client.invalidateQueries({ queryKey: ['mirrors'] }) },
    onError: (e) => toast.error(e.message),
  })
  return (
    <div>
      <PageHeader
        title="Mirrors"
        description="Offsite copies of everything the master holds, as plain files you can restore with any tool. Without one, the master is the only copy."
        actions={<AddMirror />}
      />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-40" /> : (
        <JobList
          kind="mirrors"
          jobs={data}
          emptyTitle="No mirrors yet"
          emptyText="Add one so that a failed disk does not lose your files."
          summary={(j) => (
            <>
              {String(j.detail.remote_type)}:{String(j.detail.remote_path || '')} · keeps {String(j.detail.keep_runs)} runs
              {j.detail.files !== null && <> · last run {count(Number(j.detail.files))} files, {bytes(Number(j.detail.bytes))}</>}
            </>
          )}
          extraActions={(j) => (
            <Button variant="outline" size="icon" className="size-8" aria-label="Re-send everything" onClick={() => setResync(j.name)}><RotateCcw className="size-4" /></Button>
          )}
        />
      )}
      <Confirm
        open={resync !== null}
        title={`Re-send everything to ${resync ?? ''}?`}
        description="Use this after emptying the remote. The next run uploads every file again."
        action="Re-send"
        onCancel={() => setResync(null)}
        onConfirm={() => { if (resync) resyncNow.mutate(resync); setResync(null) }}
      />
    </div>
  )
}
