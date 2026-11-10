import { useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, X } from 'lucide-react'
import { useMemo, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { ActivityView, ErrorNote, PageHeader, StatusDot } from '@/components/common'
import { DataTable, TableToolbar } from '@/components/data-table'
import { matches, useTableView } from '@/lib/table-view'
import { EventList } from '@/components/event-list'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { useNow } from '@/hooks/use-now'
import { api } from '@/lib/api'
import { ago, bytes, count, duration } from '@/lib/format'
import { locale, msg, useT } from '@/lib/i18n'
import { filesLink } from '@/lib/links'
import { useApiMutation } from '@/lib/mutation'
import { deviceAddress } from '@/lib/session'
import { STATUS } from '@/lib/status'
import type { Dashboard as DashboardData } from '@/lib/types'
import { cn } from '@/lib/utils'

const ALERT: Record<string, string> = {
  device_offline: msg('Device offline'),
  upload_error: msg('Upload error'),
  disk_usage: msg('Disk usage'),
  source_failed: msg('Source failed'),
  mirror_failed: msg('Mirror failed'),
  device_conflict: msg('Device used by two computers'),
  deletions_held: msg('Deletions held back'),
  misplaced_files: msg('Files outside a device folder'),
}

const KIND = { device: msg('Device'), local: msg('Local folder'), master: msg('Master folder') }

/** A titled block of the page: a hairline-framed panel with a header strip, not a card. */
function Panel({ title, aside, children, className }: { title: string; aside?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <section className={cn('min-w-0 overflow-hidden rounded-xl border bg-card', className)}>
      <header className="flex h-10 items-center justify-between gap-2 border-b px-4">
        <h2 className="text-sm font-semibold">{title}</h2>
        {aside && <div className="num text-[11px] text-muted-foreground">{aside}</div>}
      </header>
      {children}
    </section>
  )
}

/** One figure of the summary strip: a label over a mono value. */
function Figure({ label, value, note, tone }: { label: string; value: string; note?: string; tone?: 'warning' | 'critical' }) {
  return (
    <div className="min-w-0 px-4 py-3">
      <p className="text-xs font-medium text-muted-foreground">{label}</p>
      <p className={cn('num text-2xl leading-tight font-semibold', tone === 'warning' && 'text-warning', tone === 'critical' && 'text-destructive')}>{value}</p>
      {note && <p className="num truncate text-[11px] text-muted-foreground">{note}</p>}
    </div>
  )
}

/** The master disk as one bar: each device's stored bytes in turn, then the rest of what is used, then free; a tick marks 80 %. */
function DiskBar({ data }: { data: DashboardData }) {
  const t = useT()
  const disk = data.disk
  if (!disk) return null
  const devices = [...data.devices].filter((d) => d.bytes > 0).sort((a, b) => b.bytes - a.bytes)
  const shown = devices.slice(0, 6)
  const known = shown.reduce((n, d) => n + d.bytes, 0)
  const other = Math.max(0, disk.used - known)
  const pct = (n: number) => `${(100 * n) / disk.total}%`
  const hot = disk.percent >= 90 ? 'critical' : disk.percent >= 80 ? 'warning' : 'ok'
  // Steps of one grey, so no device looks more important than another; the bar's colour only changes when the disk is filling up.
  const tone = hot === 'critical' ? 'var(--destructive)' : hot === 'warning' ? 'var(--warning)' : 'var(--primary)'
  return (
    <div className="px-4 pt-1 pb-4">
      <div className="relative h-3 overflow-hidden rounded-full bg-muted" role="img" aria-label={t('{free} free of {total}', { free: bytes(disk.free), total: bytes(disk.total) })}>
        <div className="flex h-full">
          {shown.map((d, i) => (
            <div key={d.folder} className="h-full border-e border-card" style={{ width: pct(d.bytes), background: tone, opacity: 0.9 - i * 0.12 }} title={`${d.folder}: ${bytes(d.bytes)}`} />
          ))}
          {other > 0 && <div className="h-full" style={{ width: pct(other), background: tone, opacity: 0.18 }} title={`${t('Other')}: ${bytes(other)}`} />}
        </div>
        <i className="absolute inset-y-0 w-px bg-warning" style={{ insetInlineStart: '80%' }} aria-hidden />
      </div>
      <div className="num mt-1.5 flex flex-wrap gap-x-4 gap-y-0.5 text-[11px] text-muted-foreground">
        {shown.map((d, i) => (
          <span key={d.folder} className="flex items-center gap-1.5"><i className="size-2 rounded-[1px]" style={{ background: tone, opacity: 0.9 - i * 0.12 }} />{d.folder} {bytes(d.bytes)}</span>
        ))}
        <span className="ms-auto">{t('{free} free of {total}', { free: bytes(disk.free), total: bytes(disk.total) })}</span>
      </div>
    </div>
  )
}

/** Changes per day for two weeks, stacked: added, updated, renamed, deleted; with a scale and the busiest day marked. */
function ActivityChart({ days }: { days: DashboardData['activity'] }) {
  const t = useT()
  const totals = days.map((d) => d.added + d.updated + d.renamed + d.deleted)
  const peak = Math.max(1, ...totals)
  const busiest = totals.indexOf(Math.max(...totals))
  const day = (n: number) => new Date(n * 86400_000).toLocaleDateString(locale(), { month: 'short', day: 'numeric', timeZone: 'UTC' })
  const LEGEND: [string, string][] = [[t('Added'), 'bg-primary'], [t('Updated'), 'bg-primary/55'], [t('Renamed'), 'bg-muted-foreground/60'], [t('Deleted'), 'bg-destructive']]
  return (
    <div className="px-3 py-3">
      <div className="flex gap-2">
        <div className="num flex h-36 flex-col justify-between text-end text-[10px] text-muted-foreground" aria-hidden>
          <span>{count(peak)}</span>
          <span>{count(Math.round(peak / 2))}</span>
          <span>0</span>
        </div>
        <div className="relative flex h-36 flex-1 items-end gap-1 border-b border-s" role="img" aria-label={t('Changes per day over the last two weeks')}>
          <div className="pointer-events-none absolute inset-x-0 top-1/2 border-t border-dashed border-border" />
          {days.map((d, i) => {
            const total = totals[i]
            return (
              <div key={d.day} className="relative flex h-full flex-1 flex-col justify-end" title={`${day(d.day)}: ${t('{added} added, {updated} updated, {renamed} renamed, {deleted} deleted', { added: d.added, updated: d.updated, renamed: d.renamed, deleted: d.deleted })}`}>
                {total > 0 && i === busiest && <span className="num absolute -top-4 start-1/2 -translate-x-1/2 text-[10px] text-muted-foreground rtl:translate-x-1/2">{count(total)}</span>}
                {total > 0 && (
                  <div className="flex flex-col-reverse overflow-hidden rounded-t-[1px]" style={{ height: `${(100 * total) / peak}%` }}>
                    <div className="bg-primary" style={{ flex: d.added }} />
                    <div className="bg-primary/55" style={{ flex: d.updated }} />
                    <div className="bg-muted-foreground/60" style={{ flex: d.renamed }} />
                    <div className="bg-destructive" style={{ flex: d.deleted }} />
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>
      <div className="num mt-1.5 flex items-center justify-between ps-7 text-[10px] text-muted-foreground">
        <span>{day(days[0].day)}</span>
        <span className="flex gap-3">
          {LEGEND.map(([label, colour]) => <span key={label} className="flex items-center gap-1"><i className={cn('size-2 rounded-[1px]', colour)} />{label}</span>)}
        </span>
        <span>{t('Today')}</span>
      </div>
    </div>
  )
}

/** Computers, local folders and master folders with what each holds and is doing; filter, columns and density as on every table. */
function DevicesTable({ data }: { data: DashboardData }) {
  const t = useT()
  const now = useNow()
  const [filter, setFilter] = useState('')
  const columns = useMemo(
    () => [
      { id: 'name', label: t('Name'), locked: true },
      { id: 'state', label: t('Status'), locked: true },
      { id: 'activity', label: t('Activity') },
      { id: 'files', label: t('Files') },
      { id: 'size', label: t('Size') },
      { id: 'share', label: t('Share') },
      { id: 'seen', label: t('Last seen') },
    ],
    [t],
  )
  const view = useTableView('dashboard-devices', columns)
  const total = data.devices.reduce((n, d) => n + d.bytes, 0)
  const rows = data.devices.filter((d) => matches(filter, d.folder, d.kind, d.os, d.hostname))
  return (
    <Panel title={t('Devices')} aside={`${data.online} / ${data.devices.filter((d) => d.kind === 'device').length} ${t('online')}`}>
      <DataTable view={view} className="rounded-none border-0" toolbar={<TableToolbar view={view} filter={filter} onFilter={setFilter} placeholder={t('Filter devices')} shown={rows.length} total={data.devices.length} />}>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t('Name')}</TableHead>
              <TableHead>{t('Status')}</TableHead>
              {view.show('activity') && <TableHead className="hidden lg:table-cell">{t('Activity')}</TableHead>}
              {view.show('files') && <TableHead className="text-end">{t('Files')}</TableHead>}
              {view.show('size') && <TableHead className="text-end">{t('Size')}</TableHead>}
              {view.show('share') && <TableHead className="hidden md:table-cell">{t('Share')}</TableHead>}
              {view.show('seen') && <TableHead className="hidden sm:table-cell text-end">{t('Last seen')}</TableHead>}
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.length === 0 && (
              <TableRow className="hover:bg-transparent"><TableCell colSpan={7} className="py-6 text-center text-muted-foreground">{data.devices.length === 0 ? t('No devices yet. Add one on the Devices page.') : t('No device matches.')}</TableCell></TableRow>
            )}
            {rows.map((d) => {
              const device = d.kind === 'device'
              const share = total > 0 ? (100 * d.bytes) / total : 0
              return (
                <TableRow key={d.folder}>
                  <TableCell>
                    <Link to={device ? `/devices/${deviceAddress(d.folder)}` : filesLink(d.folder)} className="num text-[13px] font-medium hover:underline">{d.folder}</Link>
                    {!device && <span className="ms-2 text-[11px] text-muted-foreground">{t(KIND[d.kind])}</span>}
                  </TableCell>
                  <TableCell>{device ? <StatusDot status={d.revoked ? 'revoked' : d.online ? 'online' : 'offline'} label={d.revoked ? t('Revoked') : d.online ? t('Online') : t('Offline')} /> : <span className="text-muted-foreground">—</span>}</TableCell>
                  {view.show('activity') && <TableCell className="hidden w-48 lg:table-cell">{d.activity ? <ActivityView activity={d.activity} compact /> : <span className="text-muted-foreground">—</span>}</TableCell>}
                  {view.show('files') && <TableCell className="num text-end">{count(d.files)}</TableCell>}
                  {view.show('size') && <TableCell className="num text-end">{bytes(d.bytes)}</TableCell>}
                  {view.show('share') && (
                    <TableCell className="hidden md:table-cell">
                      <div className="flex items-center gap-2">
                        <div className="h-1.5 w-12 overflow-hidden rounded-full bg-muted"><div className="h-full bg-muted-foreground" style={{ width: `${share}%` }} /></div>
                        <span className="num w-9 text-[11px] text-muted-foreground">{share < 1 && share > 0 ? '<1' : Math.round(share)}%</span>
                      </div>
                    </TableCell>
                  )}
                  {view.show('seen') && <TableCell className="num hidden text-end text-muted-foreground sm:table-cell">{device ? ago(d.last_seen, now) : '—'}</TableCell>}
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      </DataTable>
    </Panel>
  )
}

export default function Dashboard() {
  const t = useT()
  const client = useQueryClient()
  const now = useNow()
  const { data, error } = useQuery({ queryKey: ['dashboard'], queryFn: () => api.get<DashboardData>('/api/dashboard') })
  const dismiss = useApiMutation((id: string) => api.post(`/api/alerts/${id}/dismiss`), {
    onSuccess: () => client.invalidateQueries({ queryKey: ['dashboard'] }),
    toastError: true,
  })

  if (error) return <ErrorNote error={error} />
  if (!data) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-8 w-40" />
        <Skeleton className="h-20" />
        <Skeleton className="h-72" />
      </div>
    )
  }
  const clients = data.devices.filter((d) => d.kind === 'device')
  const jobs = [...data.sources.map((s) => ({ ...s, type: 'source' as const })), ...data.mirrors.map((m) => ({ ...m, type: 'mirror' as const }))]
  const failing = jobs.filter((j) => j.enabled && (j.status === 'failed' || j.status === 'partial')).length
  const disk = data.disk
  const diskTone = disk ? (disk.percent >= 90 ? 'critical' : disk.percent >= 80 ? 'warning' : undefined) : undefined
  const offline = clients.filter((d) => !d.online && !d.revoked).length

  return (
    <div className="space-y-3">
      <PageHeader title={t('Dashboard')} description={t('What the master holds, who is sending it, and anything that needs attention.')} />

      {data.alerts.length > 0 && (
        <ul className="overflow-hidden rounded-xl border border-warning/50 bg-warning/10" aria-label={t('Alerts')}>
          {data.alerts.map((a) => (
            <li key={a.id} className="flex items-start gap-2 border-b border-warning/25 px-3 py-1.5 text-[13px] last:border-b-0">
              <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-warning" />
              <div className="min-w-0 flex-1">
                <span className="font-medium">{a.key ? `${t(ALERT[a.kind] ?? a.kind.replace('_', ' '))}: ${a.key}` : t(ALERT[a.kind] ?? a.kind.replace('_', ' '))}</span>
                <span className="num ms-2 text-[11px] text-muted-foreground">{ago(a.updated_at ?? a.created_at, now)}</span>
                <p className="break-words whitespace-pre-line text-xs text-muted-foreground">{a.message}</p>
              </div>
              <Button variant="ghost" size="icon-xs" onClick={() => dismiss.mutate(a.id)} aria-label={t('Dismiss alert')}><X /></Button>
            </li>
          ))}
        </ul>
      )}

      <section className="overflow-hidden rounded-xl border bg-card" aria-label={t('Summary')}>
        <div className="grid grid-cols-2 divide-x divide-border rtl:divide-x-reverse sm:grid-cols-4">
          <Figure label={t('Devices online')} value={`${data.online}/${clients.length}`} note={offline > 0 ? t('{count} offline', { count: offline }) : clients.length === 0 ? t('none added') : undefined} tone={offline > 0 ? 'warning' : undefined} />
          <Figure label={t('Files')} value={count(data.totals.files)} note={data.totals.marked_deleted ? t('{count} deleted, waiting for retention', { count: count(data.totals.marked_deleted) }) : undefined} />
          <Figure label={t('Stored')} value={bytes(data.totals.bytes)} />
          <Figure label={t('Master disk')} value={disk ? `${disk.percent}%` : '—'} note={disk ? t('{free} free of {total}', { free: bytes(disk.free), total: bytes(disk.total) }) : undefined} tone={diskTone} />
        </div>
        <DiskBar data={data} />
      </section>

      <div className="grid items-start gap-3 xl:grid-cols-[minmax(0,1fr)_22rem]">
        <div className="min-w-0 space-y-3">
          <DevicesTable data={data} />
          <Panel title={t('Changes, last 14 days')} aside={`${count(data.activity.reduce((n, d) => n + d.added + d.updated + d.renamed + d.deleted, 0))}`}>
            <ActivityChart days={data.activity} />
          </Panel>
        </div>
        <div className="min-w-0 space-y-3">
          <Panel title={t('Sources and mirrors')} aside={failing > 0 ? t('{count} need attention', { count: failing }) : jobs.length > 0 ? t('all nominal') : undefined}>
            {jobs.length === 0 ? (
              <p className="px-3 py-4 text-[13px] text-muted-foreground">{t('None configured.')}</p>
            ) : (
              <ul className="divide-y">
                {jobs.map((j) => (
                  <li key={j.type + j.key} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 px-3 py-1.5 text-[13px]">
                    <Link to={j.type === 'source' ? '/sources' : '/mirrors'} className="num truncate hover:underline">{j.key}</Link>
                    <StatusDot status={j.enabled ? j.status : 'paused'} label={j.running ? t('Running') : j.enabled ? t(STATUS[j.status] ?? j.status) : t('Paused')} />
                    <span className="text-[11px] text-muted-foreground">{j.type === 'source' ? t('Source') : t('Mirror')} · {t('every {time}', { time: duration(j.interval_secs) })}</span>
                    <span className="num text-end text-[11px] text-muted-foreground">{ago(j.last_run_at, now)}</span>
                  </li>
                ))}
              </ul>
            )}
          </Panel>
          <Panel title={t('Recent activity')}>
            <div className="px-3 py-1">
              <EventList events={data.recent} empty={t('Nothing has been collected yet.')} showFolder />
            </div>
          </Panel>
        </div>
      </div>
    </div>
  )
}
