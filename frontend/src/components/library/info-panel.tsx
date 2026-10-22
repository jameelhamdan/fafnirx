import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Calendar, FolderOpen, MapPin, RotateCcw, Star, X } from 'lucide-react'
import { lazy, Suspense, useState, type FormEvent, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { Skeleton } from '../ui/skeleton'
import { Textarea } from '../ui/textarea'
import { api, query } from '@/lib/api'
import { bytes, clock, fromInputValue, toInputValue, whenText } from '@/lib/format'
import { DEFAULT_TILES } from '@/lib/leaflet-defaults'
import { useLibraryActions, useTags } from '@/lib/library'
import type { AssetDetail, Session } from '@/lib/types'
import { cn } from '@/lib/utils'
import type { Place } from './location-picker'

const LocationPicker = lazy(() => import('./location-picker'))

function Row({ icon: Icon, label, children, action }: { icon: typeof Calendar; label: string; children: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex items-start gap-3">
      <Icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="text-xs text-muted-foreground">{label}</p>
        <div className="text-sm break-words">{children}</div>
      </div>
      {action}
    </div>
  )
}

function Stars({ value, onChange }: { value: number | null; onChange: (v: number | null) => void }) {
  return (
    <div role="radiogroup" aria-label="Rating" className="flex gap-0.5">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          role="radio"
          aria-checked={value === n}
          aria-label={`${n} star${n === 1 ? '' : 's'}`}
          // Choosing the rating that is already set takes it away.
          onClick={() => onChange(value === n ? null : n)}
          className="rounded p-0.5 hover:bg-accent"
        >
          <Star className={cn('size-5', value !== null && n <= value ? 'fill-warning text-warning' : 'text-muted-foreground')} />
        </button>
      ))}
    </div>
  )
}

/** Keyed by the saved text by its parent, so a change made elsewhere replaces the draft. */
function Description({ id, text }: { id: number; text: string }) {
  const { edit } = useLibraryActions()
  const [draft, setDraft] = useState(text)
  return (
    <Textarea
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft.trim() !== text && edit.mutate({ id, description: draft })}
      placeholder="Add a description"
      aria-label="Description"
      rows={2}
      maxLength={5000}
      className="min-h-0 resize-none"
    />
  )
}

function Tags({ id, tags }: { id: number; tags: { id: number; name: string }[] }) {
  const { edit } = useLibraryActions()
  const known = useTags()
  const [text, setText] = useState('')
  const names = tags.map((t) => t.name)
  const add = (e: FormEvent) => {
    e.preventDefault()
    const added = text.split(',').map((t) => t.trim()).filter(Boolean)
    if (added.length === 0) return
    edit.mutate({ id, tags: [...names, ...added] })
    setText('')
  }
  return (
    <div className="space-y-2">
      {names.length > 0 && (
        <ul className="flex flex-wrap gap-1.5">
          {tags.map((t) => (
            <li key={t.id} className="flex items-center gap-1 rounded-full border bg-secondary py-0.5 pr-1 pl-2.5 text-xs">
              {t.name}
              <button type="button" aria-label={`Remove tag ${t.name}`} onClick={() => edit.mutate({ id, tags: names.filter((n) => n !== t.name) })} className="grid size-4 place-items-center rounded-full hover:bg-accent">
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={add}>
        <Input value={text} onChange={(e) => setText(e.target.value)} list={`tags-${id}`} placeholder="Add a tag and press Enter" aria-label="Add a tag" maxLength={64} className="h-8 text-sm" />
        <datalist id={`tags-${id}`}>{known.data?.filter((t) => !names.includes(t.name)).map((t) => <option key={t.id} value={t.name} />)}</datalist>
      </form>
    </div>
  )
}

function DateDialog({ detail, onClose }: { detail: AssetDetail; onClose: () => void }) {
  const { edit } = useLibraryActions()
  const [value, setValue] = useState(toInputValue(detail.when))
  const parsed = fromInputValue(value)
  const save = (e: FormEvent) => {
    e.preventDefault()
    if (parsed !== null) edit.mutate({ id: detail.id, taken_at: parsed }, { onSuccess: onClose })
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={save} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Change the date</DialogTitle>
            <DialogDescription>
              The timeline uses this date. The file itself is not changed.
              {detail.camera_when ? ` The camera said ${whenText(detail.camera_when)}.` : ' The file has no date from the camera; its own modification date is used.'}
            </DialogDescription>
          </DialogHeader>
          <Input type="datetime-local" step={1} value={value} onChange={(e) => setValue(e.target.value)} aria-label="Date and time" required />
          <DialogFooter className="sm:justify-between">
            {detail.when_edited ? (
              <Button type="button" variant="ghost" onClick={() => edit.mutate({ id: detail.id, taken_at: null }, { onSuccess: onClose })}><RotateCcw className="size-4" />Use the original date</Button>
            ) : <span />}
            <Button type="submit" disabled={parsed === null || edit.isPending}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function PlaceDialog({ detail, onClose }: { detail: AssetDetail; onClose: () => void }) {
  const { edit } = useLibraryActions()
  const client = useQueryClient()
  const tiles = client.getQueryData<Session>(['session'])?.map_tiles ?? DEFAULT_TILES
  const [place, setPlace] = useState<Place | null>(detail.lat !== null && detail.lon !== null ? { lat: detail.lat, lon: detail.lon } : null)
  const [lat, setLat] = useState(place ? String(place.lat) : '')
  const [lon, setLon] = useState(place ? String(place.lon) : '')
  const choose = (p: Place) => {
    setPlace(p)
    setLat(String(p.lat))
    setLon(String(p.lon))
  }
  const typed = (la: string, lo: string) => {
    setLat(la)
    setLon(lo)
    const [a, b] = [Number(la), Number(lo)]
    if (la.trim() && lo.trim() && Number.isFinite(a) && Number.isFinite(b) && Math.abs(a) <= 90 && Math.abs(b) <= 180) setPlace({ lat: a, lon: b })
  }
  const save = (e: FormEvent) => {
    e.preventDefault()
    if (place) edit.mutate({ id: detail.id, location: place }, { onSuccess: onClose })
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <form onSubmit={save} className="space-y-4">
          <DialogHeader>
            <DialogTitle>Change the place</DialogTitle>
            <DialogDescription>Click the map or drag the pin. The file itself is not changed.</DialogDescription>
          </DialogHeader>
          <Suspense fallback={<Skeleton className="h-64" />}><LocationPicker value={place} onChange={choose} tiles={tiles} /></Suspense>
          <div className="grid grid-cols-2 gap-2">
            <Input value={lat} onChange={(e) => typed(e.target.value, lon)} inputMode="decimal" placeholder="Latitude" aria-label="Latitude" />
            <Input value={lon} onChange={(e) => typed(lat, e.target.value)} inputMode="decimal" placeholder="Longitude" aria-label="Longitude" />
          </div>
          <DialogFooter className="sm:justify-between">
            {detail.place_edited ? (
              <Button type="button" variant="ghost" onClick={() => edit.mutate({ id: detail.id, location: null }, { onSuccess: onClose })}><RotateCcw className="size-4" />Use the original place</Button>
            ) : <span />}
            <Button type="submit" disabled={!place || edit.isPending}>Save</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

const parent = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf('/')))

/** Everything about one photo or video, with the parts people can change: rating, description, tags, date, place. */
export default function InfoPanel({ id, onClose }: { id: number; onClose: () => void }) {
  const { edit } = useLibraryActions()
  const [dialog, setDialog] = useState<'date' | 'place' | null>(null)
  const { data: d, error } = useQuery({ queryKey: ['asset', id], queryFn: () => api.get<AssetDetail>(`/api/library/${id}`) })
  const exif = d?.exif ? Object.entries(d.exif) : []

  return (
    <div className="space-y-5 p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-base font-semibold">Info</h2>
        <Button variant="ghost" size="icon" className="size-8" aria-label="Close info" onClick={onClose}><X className="size-4" /></Button>
      </div>
      {error ? <p className="text-sm text-destructive">{error.message}</p> : !d ? <Skeleton className="h-64" /> : (
        <>
          <Description key={`${d.id}:${d.description}`} id={d.id} text={d.description} />
          <Stars value={d.rating} onChange={(rating) => edit.mutate({ id: d.id, rating })} />
          <Tags id={d.id} tags={d.tags} />

          <Row icon={Calendar} label="Date" action={<Button variant="ghost" size="sm" onClick={() => setDialog('date')}>Edit</Button>}>
            {whenText(d.when)}
            {d.when_edited && <span className="ml-1.5 text-xs text-muted-foreground">(changed)</span>}
          </Row>
          <Row icon={MapPin} label="Place" action={<Button variant="ghost" size="sm" onClick={() => setDialog('place')}>{d.lat === null ? 'Add' : 'Edit'}</Button>}>
            {d.lat !== null && d.lon !== null ? <span className="tabular-nums">{d.lat.toFixed(5)}, {d.lon.toFixed(5)}{d.place_edited && <span className="ml-1.5 text-xs text-muted-foreground">(changed)</span>}</span> : <span className="text-muted-foreground">No place</span>}
          </Row>
          <Row icon={FolderOpen} label="File">
            <Link to={`/files${query({ device: d.device, path: parent(d.path), file: d.path })}`} className="hover:underline">{d.device}/{d.path}</Link>
            <p className="mt-1 text-xs text-muted-foreground">
              {bytes(d.size)}{d.width && d.height ? ` · ${d.width} × ${d.height}` : ''}{d.duration_ms ? ` · ${clock(d.duration_ms)}` : ''} · {d.mime}{d.codec ? ` · ${d.codec}` : ''}
            </p>
          </Row>

          {d.albums.length > 0 && (
            <section>
              <h3 className="mb-1.5 text-xs text-muted-foreground">In albums</h3>
              <ul className="flex flex-wrap gap-1.5">
                {d.albums.map((a) => <li key={a.id}><Link to={`/albums/${a.id}`} className="rounded-full border px-2.5 py-0.5 text-xs hover:bg-accent">{a.name}</Link></li>)}
              </ul>
            </section>
          )}

          {exif.length > 0 && (
            <section>
              <h3 className="mb-1.5 text-xs text-muted-foreground">Camera</h3>
              <dl className="grid grid-cols-2 gap-x-3 gap-y-1.5 text-xs">
                {exif.map(([k, v]) => (
                  <div key={k} className="min-w-0"><dt className="text-muted-foreground">{k.replace(/([a-z])([A-Z])/g, '$1 $2')}</dt><dd className="break-words">{v}</dd></div>
                ))}
              </dl>
            </section>
          )}
          {dialog === 'date' && <DateDialog detail={d} onClose={() => setDialog(null)} />}
          {dialog === 'place' && <PlaceDialog detail={d} onClose={() => setDialog(null)} />}
        </>
      )}
    </div>
  )
}
