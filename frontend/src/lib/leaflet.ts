import L from 'leaflet'
import 'leaflet/dist/leaflet.css'

export { L }

/** Adds the map's pictures. They come from `template` (an OpenStreetMap-style tile address), so a server
 * without internet access can point it at a tile server of its own. */
export function addTiles(map: L.Map, template: string) {
  return L.tileLayer(template, {
    maxZoom: 19,
    attribution: template.includes('openstreetmap.org') ? '© OpenStreetMap contributors' : '',
  }).addTo(map)
}
