import { keepPreviousData, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'
import { useWindowVirtualizer } from '@tanstack/react-virtual'
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { DayGrid } from '@/components/library/day-grid'
import { Scrubber } from '@/components/library/scrubber'
import { SelectionBar } from '@/components/library/selection-bar'
import { Viewer } from '@/components/library/viewer'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { api, mediaUrl, query } from '@/lib/api'
import { count, monthTitle } from '@/lib/format'
import { monthBodyHeight, monthHeight, useGrid } from '@/lib/grid'
import { monthQuery, useTags, type TimelineFilters } from '@/lib/library'
import type { Asset, Buckets, DeviceSummary, Scope } from '@/lib/types'

const TITLES: Record<Scope, string> = { timeline: 'Photos', favorites: 'Favorites', archive: 'Archive' }
const DESCRIPTIONS: Record<Scope, string> = {
  timeline: 'Every photo and video your devices have sent, newest first.',
  favorites: 'The photos and videos you marked with a heart.',
  archive: 'Hidden from the timeline, but still kept.',
}

/** The assets of several loaded months, in order. */
const combineAssets = (results: { data?: { assets: Asset[] } }[]) => results.flatMap((r) => r.data?.assets ?? [])

/** The title of a month and its days; the pictures load when the month comes near the screen. */
function MonthSection({
  month,
  size,
  bodyHeight,
  filters,
  selected,
  onOpen,
  onToggle,
  onToggleMany,
}: {
  month: string
  size: number
  /** Room the pictures will take, kept free until they are there. */
  bodyHeight: number
  filters: TimelineFilters
  selected: Map<number, Asset>
  onOpen: (month: string, a: Asset) => void
  onToggle: (a: Asset) => void
  onToggleMany: (a: Asset[], on: boolean) => void
}) {
  const { data, error } = useQuery(monthQuery(filters, month))
  const ids = useMemo(() => new Set(selected.keys()), [selected])
  const byId = useMemo(() => new Map((data?.assets ?? []).map((a) => [a.id, a])), [data])
  return (
    <section aria-label={monthTitle(month)} className="pb-2">
      <h2 className="mb-1 py-2 text-base font-semibold">
        {monthTitle(month)} <span className="ml-1 text-xs font-normal text-muted-foreground">{count(size)}</span>
      </h2>
      {error ? (
        <ErrorNote error={error} />
      ) : !data ? (
        <Skeleton className="rounded-md" style={{ height: bodyHeight }} />
      ) : (
        <DayGrid
          assets={data.assets}
          src={(a) => mediaUrl(a.id, 'thumb', a.hash)}
          selected={ids}
          onOpen={(a) => onOpen(month, a as Asset)}
          onToggle={(id) => { const a = byId.get(id); if (a) onToggle(a) }}
          onToggleMany={(list, on) => onToggleMany(list.map((id) => byId.get(id)).filter((a): a is Asset => !!a), on)}
        />
      )}
    </section>
  )
}

/** The photo library as a timeline: months of days of pictures, with a scrubber to jump through time. */
export default function Timeline({ scope }: { scope: Scope }) {
  const client = useQueryClient()
  const [params, setParams] = useSearchParams()
  const filters: TimelineFilters = { scope, device: params.get('device') ?? '', kind: params.get('kind') ?? '', tag: params.get('tag') ?? '' }
  const setFilter = (key: 'device' | 'kind' | 'tag', value: string) => {
    const next = new URLSearchParams(params)
    if (value && value !== 'all') next.set(key, value)
    else next.delete(key)
    setParams(next, { replace: true })
    setSelected(new Map())
    window.scrollTo({ top: 0 })
  }

  const buckets = useQuery({
    queryKey: ['timeline', 'buckets', filters],
    queryFn: () => api.get<Buckets>(`/api/timeline/buckets${query(filters)}`),
    placeholderData: keepPreviousData,
  })
  const devices = useQuery({ queryKey: ['devices'], queryFn: () => api.get<DeviceSummary[]>('/api/devices') })
  const tags = useTags()
  const months = useMemo(() => buckets.data?.months ?? [], [buckets.data])

  // Selecting.
  const [selected, setSelected] = useState<Map<number, Asset>>(new Map())
  const toggle = useCallback((a: Asset) => setSelected((s) => { const n = new Map(s); if (!n.delete(a.id)) n.set(a.id, a); return n }), [])
  const toggleMany = useCallback((list: Asset[], on: boolean) => setSelected((s) => { const n = new Map(s); list.forEach((a) => (on ? n.set(a.id, a) : n.delete(a.id))); return n }), [])
  const clear = useCallback(() => setSelected(new Map()), [])

  // The list of months, drawn only where it is on screen.
  const listRef = useRef<HTMLDivElement>(null)
  const { cols, cell } = useGrid(listRef)
  const [margin, setMargin] = useState(0)
  useLayoutEffect(() => {
    // Where the list begins on the page: it moves when the heading above wraps, so watch the page's size.
    const measure = () => {
      const el = listRef.current
      if (el) setMargin(Math.round(el.getBoundingClientRect().top + window.scrollY))
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(document.body)
    return () => observer.disconnect()
  }, [months.length, cols])
  const virtual = useWindowVirtualizer({
    count: months.length,
    // The server says how many files each day holds, so a month's height is known before its pictures load.
    estimateSize: (i) => monthHeight(months[i].days, cols, cell),
    overscan: 2,
    scrollMargin: margin,
    getItemKey: (i) => months[i].month,
  })
  const starts = months.map((_, i) => (virtual.measurementsCache[i]?.start ?? margin) - margin)

  // The viewer: the months around the one that was opened, loaded as far as you page.
  const [viewer, setViewer] = useState<{ months: string[]; id: number } | null>(null)
  const viewedAssets = useQueries({ queries: (viewer?.months ?? []).map((m) => monthQuery(filters, m)), combine: combineAssets })
  const extending = useRef(false)
  const extend = useCallback(
    async (direction: 'prev' | 'next') => {
      if (!viewer || extending.current) return
      const at = months.findIndex((m) => m.month === (direction === 'next' ? viewer.months[viewer.months.length - 1] : viewer.months[0]))
      const target = months[direction === 'next' ? at + 1 : at - 1]
      if (at < 0 || !target) return
      extending.current = true
      try {
        await client.fetchQuery(monthQuery(filters, target.month))
        setViewer((v) => (v && !v.months.includes(target.month) ? { ...v, months: direction === 'next' ? [...v.months, target.month] : [target.month, ...v.months] } : v))
      } finally {
        extending.current = false
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [viewer, months, client, filters.scope, filters.device, filters.kind, filters.tag],
  )

  const total = buckets.data?.total ?? 0
  const filtering = filters.device !== '' || filters.kind !== '' || filters.tag !== ''
  const selectedList = [...selected.values()]

  return (
    <div className="md:pr-9">
      <PageHeader title={TITLES[scope]} description={buckets.data ? `${count(total)} photo${total === 1 ? '' : 's'} and video${total === 1 ? '' : 's'}. ${DESCRIPTIONS[scope]}` : DESCRIPTIONS[scope]} />

      <div className="sticky top-14 z-20 -mx-3 mb-2 bg-background/90 px-3 py-2 backdrop-blur md:-mx-6 md:px-6">
        {selected.size > 0 ? (
          <SelectionBar selected={selectedList} onClear={clear} />
        ) : (
          <div className="flex h-11 flex-wrap items-center gap-2 overflow-hidden">
            <Select value={filters.device || 'all'} onValueChange={(v) => setFilter('device', v)}>
              <SelectTrigger className="w-40" aria-label="Device"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">All devices</SelectItem>
                {devices.data?.filter((d) => d.files > 0).map((d) => <SelectItem key={d.name} value={d.name}>{d.name}</SelectItem>)}
              </SelectContent>
            </Select>
            <Select value={filters.kind || 'all'} onValueChange={(v) => setFilter('kind', v)}>
              <SelectTrigger className="w-44" aria-label="Type"><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="all">Photos and videos</SelectItem>
                <SelectItem value="image">Photos</SelectItem>
                <SelectItem value="video">Videos</SelectItem>
              </SelectContent>
            </Select>
            {tags.data && tags.data.length > 0 && (
              <Select value={filters.tag || 'all'} onValueChange={(v) => setFilter('tag', v)}>
                <SelectTrigger className="w-36" aria-label="Tag"><SelectValue /></SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">Any tag</SelectItem>
                  {tags.data.map((t) => <SelectItem key={t.id} value={String(t.id)}>{t.name} ({t.count})</SelectItem>)}
                </SelectContent>
              </Select>
            )}
          </div>
        )}
      </div>

      <div ref={listRef}>
        {buckets.error ? <ErrorNote error={buckets.error} /> : !buckets.data ? (
          <Skeleton className="h-96" />
        ) : months.length === 0 ? (
          filtering ? (
            <Empty title="Nothing matches">Try another device, type or tag.</Empty>
          ) : scope === 'timeline' ? (
            <Empty title="No photos or videos yet">Photos and videos appear here as devices send them. Back up a folder with the fafnirx client, or upload into a local folder on the Files page.</Empty>
          ) : scope === 'favorites' ? (
            <Empty title="No favorites yet">Select photos and choose Favorite, or press the heart while viewing one.</Empty>
          ) : (
            <Empty title="The archive is empty">Archived photos leave the timeline but are kept. Select photos and choose Archive.</Empty>
          )
        ) : (
          <div className="relative" style={{ height: virtual.getTotalSize() }}>
            {virtual.getVirtualItems().map((item) => {
              const m = months[item.index]
              return (
                <div
                  key={item.key}
                  data-index={item.index}
                  ref={virtual.measureElement}
                  className="absolute top-0 left-0 w-full"
                  style={{ transform: `translateY(${item.start - virtual.options.scrollMargin}px)` }}
                >
                  <MonthSection
                    month={m.month}
                    size={m.count}
                    bodyHeight={monthBodyHeight(m.days, cols, cell)}
                    filters={filters}
                    selected={selected}
                    onOpen={(month, a) => setViewer({ months: [month], id: a.id })}
                    onToggle={toggle}
                    onToggleMany={toggleMany}
                  />
                </div>
              )
            })}
          </div>
        )}
      </div>

      <Scrubber months={months} starts={starts} total={virtual.getTotalSize()} scrollMargin={margin} />

      {viewer && (
        <Viewer
          assets={viewedAssets}
          id={viewer.id}
          onChange={(id) => setViewer((v) => (v ? { ...v, id } : v))}
          onClose={() => setViewer(null)}
          onEdge={extend}
        />
      )}
    </div>
  )
}
