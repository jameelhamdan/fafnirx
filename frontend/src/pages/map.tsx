import { useQuery, useQueryClient } from '@tanstack/react-query'
import Supercluster from 'supercluster'
import { useEffect, useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'
import { Empty, ErrorNote, PageHeader } from '@/components/common'
import { Thumb } from '@/components/library/thumb'
import { Viewer } from '@/components/library/viewer'
import { Skeleton } from '@/components/ui/skeleton'
import { api, mediaUrl } from '@/lib/api'
import { count } from '@/lib/format'
import { DEFAULT_TILES } from '@/lib/leaflet-defaults'
import { addTiles, L } from '@/lib/leaflet'
import type { Asset, MapData, Session } from '@/lib/types'

type Props = { id: number; when: number }
const STRIP = 60

/** A round picture of a photo, or a numbered bubble for a group of them. */
const pin = (id: number) =>
  L.divIcon({
    className: '',
    iconSize: [46, 46],
    html: `<img src="${mediaUrl(id, 'thumb')}" alt="" draggable="false" style="width:46px;height:46px;border-radius:50%;object-fit:cover;border:3px solid #fff;box-shadow:0 1px 6px rgba(0,0,0,.45);background:#888">`,
  })

const bubble = (n: number) => {
  const size = Math.min(64, 34 + Math.round(Math.log10(n) * 10))
  return L.divIcon({
    className: '',
    iconSize: [size, size],
    html: `<div style="width:${size}px;height:${size}px;border-radius:50%;display:grid;place-items:center;background:rgba(79,70,229,.9);color:#fff;font:600 13px system-ui;border:3px solid rgba(255,255,255,.9);box-shadow:0 1px 6px rgba(0,0,0,.45)">${n >= 1000 ? `${Math.round(n / 100) / 10}k` : n}</div>`,
  })
}

/** Every photo and video that has a place, on a map. Groups open as you zoom; the strip below lists what is in view. */
export default function MapPage() {
  const client = useQueryClient()
  const tiles = client.getQueryData<Session>(['session'])?.map_tiles ?? DEFAULT_TILES
  const { data, error } = useQuery({ queryKey: ['map'], queryFn: () => api.get<MapData>('/api/map') })
  const host = useRef<HTMLDivElement>(null)
  const map = useRef<L.Map | null>(null)
  const markers = useRef<L.LayerGroup | null>(null)
  const fitted = useRef(false)
  const [inView, setInView] = useState<{ ids: number[]; total: number }>({ ids: [], total: 0 })
  const [viewer, setViewer] = useState<{ assets: Asset[]; id: number } | null>(null)

  const points = data?.points
  const index = useMemo(() => {
    if (!points) return null
    const sc = new Supercluster<Props>({ radius: 70, maxZoom: 17 })
    sc.load(points.map(([id, lat, lon, when]) => ({ type: 'Feature', properties: { id, when }, geometry: { type: 'Point', coordinates: [lon, lat] } })))
    return sc
  }, [points])

  const open = async (ids: number[], start: number) => {
    try {
      const { assets } = await api.get<{ assets: Asset[] }>(`/api/library?ids=${ids.slice(0, 500).join(',')}`)
      if (assets.length > 0) setViewer({ assets, id: assets.some((a) => a.id === start) ? start : assets[0].id })
    } catch (e) {
      toast.error(e instanceof Error ? e.message : 'Could not open it')
    }
  }
  const openRef = useRef(open)
  useEffect(() => {
    openRef.current = open
  })

  const hasPlaces = (points?.length ?? 0) > 0
  // The map is made once, when there is something to show.
  useEffect(() => {
    const el = host.current
    if (!el || !hasPlaces) return
    const m = L.map(el, { center: [25, 10], zoom: 2, minZoom: 2, worldCopyJump: true })
    addTiles(m, tiles)
    markers.current = L.layerGroup().addTo(m)
    map.current = m
    return () => {
      m.remove()
      map.current = null
      markers.current = null
      fitted.current = false
    }
  }, [hasPlaces, tiles])

  // What is drawn follows the data and the view.
  useEffect(() => {
    const m = map.current
    const layer = markers.current
    if (!m || !layer || !index || !points) return
    if (!fitted.current) {
      fitted.current = true
      const bounds = L.latLngBounds(points.map(([, lat, lon]) => [lat, lon] as [number, number]))
      m.fitBounds(bounds, { padding: [40, 40], maxZoom: 14 })
    }
    const draw = () => {
      const b = m.getBounds()
      const west = Math.max(-180, b.getWest())
      const east = Math.min(180, b.getEast())
      const zoom = Math.round(m.getZoom())
      layer.clearLayers()
      for (const f of index.getClusters([west, Math.max(-85, b.getSouth()), east, Math.min(85, b.getNorth())], zoom)) {
        const [lon, lat] = f.geometry.coordinates
        if ('cluster' in f.properties && f.properties.cluster) {
          const clusterId = f.properties.cluster_id
          L.marker([lat, lon], { icon: bubble(f.properties.point_count), keyboard: false })
            .on('click', () => {
              const zoomTo = index.getClusterExpansionZoom(clusterId)
              // Points that stay together even when fully zoomed in (the same spot): show them instead.
              if (zoomTo > 18 || m.getZoom() >= 18) void openRef.current(index.getLeaves(clusterId, 200).map((l) => l.properties.id), 0)
              else m.flyTo([lat, lon], Math.min(zoomTo, 18))
            })
            .addTo(layer)
        } else {
          const id = (f.properties as Props).id
          L.marker([lat, lon], { icon: pin(id), keyboard: false }).on('click', () => void openRef.current([id, ...visible().filter((v) => v !== id)], id)).addTo(layer)
        }
      }
      const seen = visible()
      setInView({ ids: seen.slice(0, STRIP), total: seen.length })
    }
    /** Points inside the view, newest first. */
    const visible = () => {
      const b = m.getBounds()
      return points
        .filter(([, lat, lon]) => b.contains([lat, lon]))
        .sort((a, c) => c[3] - a[3])
        .map((p) => p[0])
    }
    draw()
    m.on('moveend', draw)
    return () => {
      m.off('moveend', draw)
    }
  }, [index, points, hasPlaces])

  return (
    <div>
      <PageHeader title="Map" description={points ? `${count(points.length)} photo${points.length === 1 ? '' : 's'} and videos with a place${data?.truncated ? ' (the newest are shown)' : ''}. Places come from the camera, or from ones you set.` : undefined} />
      {error ? <ErrorNote error={error} /> : !data ? (
        <Skeleton className="h-96" />
      ) : !hasPlaces ? (
        <Empty title="No photos with a place yet">Photos taken with location turned on show up here. You can also give any photo a place from its info panel.</Empty>
      ) : (
        <>
          <div ref={host} className="isolate h-[calc(100svh-17rem)] min-h-80 overflow-hidden rounded-lg border bg-muted" role="application" aria-label="Map of your photos" />
          <section aria-label="In view" className="mt-3">
            <p className="mb-1.5 text-sm text-muted-foreground">{inView.total === 0 ? 'Nothing in view. Move or zoom the map.' : `${count(inView.total)} in view${inView.total > STRIP ? `, the newest ${STRIP} below` : ''}`}</p>
            <ul className="flex gap-1.5 overflow-x-auto pb-2">
              {inView.ids.map((id) => (
                <li key={id} className="shrink-0">
                  <button type="button" onClick={() => void open(inView.ids, id)} className="block size-20 overflow-hidden rounded-md bg-muted" aria-label="Open photo">
                    <Thumb src={mediaUrl(id, 'thumb')} className="size-full" fallback={null} />
                  </button>
                </li>
              ))}
            </ul>
          </section>
        </>
      )}
      {viewer && <Viewer assets={viewer.assets} id={viewer.id} onChange={(id) => setViewer((v) => (v ? { ...v, id } : v))} onClose={() => setViewer(null)} />}
    </div>
  )
}
