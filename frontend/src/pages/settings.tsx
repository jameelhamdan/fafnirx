import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { ErrorNote, PageHeader } from '@/components/common'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Skeleton } from '@/components/ui/skeleton'
import { api } from '@/lib/api'
import type { Settings as SettingsData } from '@/lib/types'

function SettingsForm({ data }: { data: SettingsData }) {
  const client = useQueryClient()
  const [versions, setVersions] = useState(String(data.version_count))
  const [days, setDays] = useState(String(data.retention_days))
  const save = useMutation({
    mutationFn: () => api.put<SettingsData>('/api/settings', { version_count: Number(versions), retention_days: Number(days) }),
    onSuccess: (saved) => { client.setQueryData(['settings'], saved); toast.success('Settings saved') },
    onError: (e) => toast.error(e.message),
  })
  const submit = (e: FormEvent) => { e.preventDefault(); save.mutate() }
  return (
    <Card className="max-w-xl">
      <CardHeader>
        <CardTitle>Versions and retention</CardTitle>
        <CardDescription>
          When a file changes, the old content is kept as a version. A deleted file, and versions beyond the count, are removed for good once the retention period has passed.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="versions">Versions kept per file</Label>
            <Input id="versions" type="number" min="1" max="1000" value={versions} onChange={(e) => setVersions(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="days">Retention (days)</Label>
            <Input id="days" type="number" min="1" max="3650" value={days} onChange={(e) => setDays(e.target.value)} />
          </div>
          <Button type="submit" disabled={save.isPending}>Save</Button>
        </form>
      </CardContent>
    </Card>
  )
}

export default function Settings() {
  const { data, error } = useQuery({ queryKey: ['settings'], queryFn: () => api.get<SettingsData>('/api/settings') })
  return (
    <div>
      <PageHeader title="Settings" />
      {error ? <ErrorNote error={error} /> : !data ? <Skeleton className="h-56 max-w-xl" /> : <SettingsForm data={data} />}
    </div>
  )
}
