import { useEffect, useRef } from 'react'
import { addTiles, L } from '@/lib/leaflet'

export interface Place {
  lat: number
  lon: number
}

/** A small map: click it, or drag the pin, to choose a place. */
export default function LocationPicker({ value, onChange, tiles }: { value: Place | null; onChange: (p: Place) => void; tiles: string }) {
  const host = useRef<HTMLDivElement>(null)
  const map = useRef<L.Map | null>(null)
  const pin = useRef<L.Marker | null>(null)
  const notify = useRef(onChange)
  useEffect(() => {
    notify.current = onChange
  })

  useEffect(() => {
    const el = host.current
    if (!el) return
    const m = L.map(el, { center: value ? [value.lat, value.lon] : [25, 10], zoom: value ? 12 : 2, worldCopyJump: true })
    addTiles(m, tiles)
    m.on('click', (e: L.LeafletMouseEvent) => notify.current({ lat: round(e.latlng.lat), lon: round(wrap(e.latlng.lng)) }))
    map.current = m
    // The map is drawn inside a dialog that is still opening: measure again once it has its size.
    const late = window.setTimeout(() => m.invalidateSize(), 250)
    return () => {
      window.clearTimeout(late)
      m.remove()
      map.current = null
      pin.current = null
    }
    // The map is made once; the pin follows `value` below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tiles])

  useEffect(() => {
    const m = map.current
    if (!m) return
    if (!value) {
      pin.current?.remove()
      pin.current = null
      return
    }
    const at: L.LatLngExpression = [value.lat, value.lon]
    if (pin.current) pin.current.setLatLng(at)
    else {
      pin.current = L.marker(at, { draggable: true, icon: L.divIcon({ className: '', html: PIN, iconSize: [26, 34], iconAnchor: [13, 34] }) }).addTo(m)
      pin.current.on('dragend', () => {
        const p = pin.current?.getLatLng()
        if (p) notify.current({ lat: round(p.lat), lon: round(wrap(p.lng)) })
      })
    }
    if (!m.getBounds().contains(at)) m.panTo(at)
  }, [value])

  return <div ref={host} className="isolate h-64 w-full overflow-hidden rounded-lg border" role="application" aria-label="Map: click to choose a place" />
}

const round = (n: number) => Math.round(n * 1e6) / 1e6
/** A map that was dragged around the world reports longitudes beyond ±180. */
const wrap = (lon: number) => ((((lon + 180) % 360) + 360) % 360) - 180

const PIN =
  '<svg width="26" height="34" viewBox="0 0 26 34" xmlns="http://www.w3.org/2000/svg"><path d="M13 0C5.8 0 0 5.8 0 13c0 9.7 13 21 13 21s13-11.3 13-21C26 5.8 20.2 0 13 0z" fill="#4f46e5" stroke="#fff" stroke-width="2"/><circle cx="13" cy="13" r="4.5" fill="#fff"/></svg>'
