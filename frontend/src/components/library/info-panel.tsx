import { useQuery } from '@tanstack/react-query'
import { Calendar, FolderOpen, MapPin, RotateCcw, Star, X } from 'lucide-react'
import { useState, type FormEvent, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { Button } from '../ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '../ui/dialog'
import { Input } from '../ui/input'
import { Skeleton } from '../ui/skeleton'
import { Textarea } from '../ui/textarea'
import { api, query } from '@/lib/api'
import { bytes, clock, fromInputValue, toInputValue, whenText } from '@/lib/format'
import { useLibraryActions, useTags } from '@/lib/library'
import { hasWorkspace, useRole } from '@/lib/session'
import type { AssetDetail } from '@/lib/types'
import { cn } from '@/lib/utils'
import { WorkNote } from '../work-note'
import { useT } from '@/lib/i18n'

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
  const t = useT()
  return (
    <div role="radiogroup" aria-label={t('Rating')} className="flex gap-0.5">
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          role="radio"
          aria-checked={value === n}
          aria-label={n === 1 ? t('1 star') : t('{n} stars', { n })}
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
function Description({ id, text, fromFile }: { id: string; text: string; fromFile: string | null }) {
  const t = useT()
  const { edit } = useLibraryActions()
  const [draft, setDraft] = useState(text)
  return (
    <div className="space-y-1.5">
      <Textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => draft.trim() !== text && edit.mutate({ id, description: draft })}
        placeholder={t('Add a description')}
        aria-label={t('Description')}
        rows={2}
        maxLength={5000}
        className="min-h-0 resize-none"
      />
      {fromFile && fromFile !== text && (
        <p className="text-xs text-muted-foreground">
          {t('In the file:')} {fromFile}{!text && <> · <button type="button" className="underline" onClick={() => edit.mutate({ id, description: fromFile })}>{t('Use it')}</button></>}
        </p>
      )}
    </div>
  )
}

function Tags({ id, tags, suggested }: { id: string; tags: { id: string; name: string }[]; suggested: string[] }) {
  const t = useT()
  const { edit } = useLibraryActions()
  const known = useTags()
  const [text, setText] = useState('')
  const names = tags.map((tag) => tag.name)
  const add = (e: FormEvent) => {
    e.preventDefault()
    const added = text.split(',').map((s) => s.trim()).filter(Boolean)
    if (added.length === 0) return
    edit.mutate({ id, tags: [...names, ...added] })
    setText('')
  }
  return (
    <div className="space-y-2">
      {names.length > 0 && (
        <ul className="flex flex-wrap gap-1.5">
          {tags.map((tag) => (
            <li key={tag.id} className="flex items-center gap-1 rounded-full border bg-secondary py-0.5 pr-1 pl-2.5 text-xs">
              {tag.name}
              <button type="button" aria-label={t('Remove tag {name}', { name: tag.name })} onClick={() => edit.mutate({ id, tags: names.filter((n) => n !== tag.name) })} className="grid size-4 place-items-center rounded-full hover:bg-accent">
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}
      {suggested.some((n) => !names.some((x) => x.toLowerCase() === n.toLowerCase())) && (
        <div>
          <p className="mb-1 text-xs text-muted-foreground">{t('Keywords in the file, click to add')}</p>
          <ul className="flex flex-wrap gap-1.5">
            {suggested.filter((n) => !names.some((x) => x.toLowerCase() === n.toLowerCase())).map((n) => (
              <li key={n}><button type="button" onClick={() => edit.mutate({ id, tags: [...names, n] })} className="rounded-full border border-dashed px-2.5 py-0.5 text-xs hover:bg-accent">{n}</button></li>
            ))}
          </ul>
        </div>
      )}
      <form onSubmit={add}>
        <Input value={text} onChange={(e) => setText(e.target.value)} list={`tags-${id}`} placeholder={t('Add a tag and press Enter')} aria-label={t('Add a tag')} maxLength={64} className="h-8 text-sm" />
        <datalist id={`tags-${id}`}>{known.data?.filter((tag) => !names.includes(tag.name)).map((tag) => <option key={tag.id} value={tag.name} />)}</datalist>
      </form>
    </div>
  )
}

function DateDialog({ detail, onClose }: { detail: AssetDetail; onClose: () => void }) {
  const t = useT()
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
            <DialogTitle>{t('Change the date')}</DialogTitle>
            <DialogDescription>
              {t('The timeline uses this date. The file itself is not changed.')}{' '}
              {detail.camera_when ? t('The camera said {when}.', { when: whenText(detail.camera_when) }) : t('The file has no date from the camera; its own modification date is used.')}
            </DialogDescription>
          </DialogHeader>
          <Input type="datetime-local" step={1} value={value} onChange={(e) => setValue(e.target.value)} aria-label={t('Date and time')} required />
          <DialogFooter className="sm:justify-between">
            {detail.when_edited ? (
              <Button type="button" variant="ghost" onClick={() => edit.mutate({ id: detail.id, taken_at: null }, { onSuccess: onClose })}><RotateCcw className="size-4" />{t('Use the original date')}</Button>
            ) : <span />}
            <Button type="submit" disabled={parsed === null || edit.isPending}>{t('Save')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** A place as latitude and longitude, or null when the text is not one. */
function placeOf(lat: string, lon: string): { lat: number; lon: number } | null {
  const [a, b] = [Number(lat), Number(lon)]
  if (!lat.trim() || !lon.trim() || !Number.isFinite(a) || !Number.isFinite(b) || Math.abs(a) > 90 || Math.abs(b) > 180) return null
  return { lat: a, lon: b }
}

function PlaceDialog({ detail, onClose }: { detail: AssetDetail; onClose: () => void }) {
  const t = useT()
  const { edit } = useLibraryActions()
  const [lat, setLat] = useState(detail.lat === null ? '' : String(detail.lat))
  const [lon, setLon] = useState(detail.lon === null ? '' : String(detail.lon))
  const place = placeOf(lat, lon)
  const save = (e: FormEvent) => {
    e.preventDefault()
    if (place) edit.mutate({ id: detail.id, location: place }, { onSuccess: onClose })
  }
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <form onSubmit={save} className="space-y-4">
          <DialogHeader>
            <DialogTitle>{t('Change the place')}</DialogTitle>
            <DialogDescription>{t('Enter the latitude and longitude. The file itself is not changed.')}</DialogDescription>
          </DialogHeader>
          <div className="grid grid-cols-2 gap-2">
            <Input value={lat} onChange={(e) => setLat(e.target.value)} inputMode="decimal" placeholder={t('Latitude')} aria-label={t('Latitude')} />
            <Input value={lon} onChange={(e) => setLon(e.target.value)} inputMode="decimal" placeholder={t('Longitude')} aria-label={t('Longitude')} />
          </div>
          <DialogFooter className="sm:justify-between">
            {detail.place_edited ? (
              <Button type="button" variant="ghost" onClick={() => edit.mutate({ id: detail.id, location: null }, { onSuccess: onClose })}><RotateCcw className="size-4" />{t('Use the original place')}</Button>
            ) : <span />}
            <Button type="submit" disabled={!place || edit.isPending}>{t('Save')}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

const parent = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf('/')))

/** Everything about one photo or video, with the parts people can change: rating, description, tags, date, place. */
export default function InfoPanel({ id, onClose, embedded }: { id: string; onClose?: () => void; embedded?: boolean }) {
  const t = useT()
  const { edit } = useLibraryActions()
  // A read-only user sees everything about a file and changes nothing.
  const mine = hasWorkspace(useRole())
  const [dialog, setDialog] = useState<'date' | 'place' | null>(null)
  const { data: d, error } = useQuery({ queryKey: ['asset', id], queryFn: () => api.get<AssetDetail>(`/api/library/${id}`) })
  const exif = d?.exif ? Object.entries(d.exif) : []

  return (
    <div className={cn('space-y-5', !embedded && 'p-4')}>
      {!embedded && (
        <div className="flex items-center justify-between">
          <h2 className="text-base font-semibold">{t('Info')}</h2>
          {onClose && <Button variant="ghost" size="icon" className="size-8" aria-label={t('Close info')} onClick={onClose}><X className="size-4" /></Button>}
        </div>
      )}
      {error ? <p className="text-sm text-destructive">{error.message}</p> : !d ? <Skeleton className="h-64" /> : (
        <>
          <WorkNote work={d.work} />
          {mine ? (
            <>
              <Description key={`${d.id}:${d.description}`} id={d.id} text={d.description} fromFile={d.file_description} />
              <Stars value={d.rating} onChange={(rating) => edit.mutate({ id: d.id, rating })} />
              <Tags id={d.id} tags={d.tags} suggested={d.file_tags} />
            </>
          ) : null}

          <Row icon={Calendar} label={t('Date')} action={mine ? <Button variant="ghost" size="sm" onClick={() => setDialog('date')}>{t('Edit')}</Button> : undefined}>
            {whenText(d.when)}
            {d.when_edited && <span className="ml-1.5 text-xs text-muted-foreground">{t('(changed)')}</span>}
          </Row>
          <Row icon={MapPin} label={t('Place')} action={mine ? <Button variant="ghost" size="sm" onClick={() => setDialog('place')}>{d.lat === null ? t('Add') : t('Edit')}</Button> : undefined}>
            {d.lat !== null && d.lon !== null ? <span className="tabular-nums">{d.lat.toFixed(5)}, {d.lon.toFixed(5)}{d.place_edited && <span className="ml-1.5 text-xs text-muted-foreground">{t('(changed)')}</span>}</span> : <span className="text-muted-foreground">{t('No place')}</span>}
          </Row>
          <Row icon={FolderOpen} label={t('File')}>
            <Link to={`/files${query({ device: d.device, path: parent(d.path), file: d.path })}`} className="hover:underline">{d.device}/{d.path}</Link>
            <p className="mt-1 text-xs text-muted-foreground">
              {bytes(d.size)}{d.width && d.height ? ` · ${d.width} × ${d.height}` : ''}{d.duration_ms ? ` · ${clock(d.duration_ms)}` : ''} · {d.mime}{d.codec ? ` · ${d.codec}` : ''}
            </p>
          </Row>

          {d.described && (
            <section className="space-y-1">
              <h3 className="text-xs text-muted-foreground">{d.described.show ?? t('Description')}</h3>
              <p className="text-sm font-medium">
                {d.described.season !== null && d.described.episode !== null && <span className="mr-1.5 tabular-nums text-muted-foreground">S{String(d.described.season).padStart(2, '0')}E{String(d.described.episode).padStart(2, '0')}</span>}
                {d.described.title}
              </p>
              {d.described.aired && <p className="text-xs text-muted-foreground">{d.described.aired}</p>}
              {d.described.plot && <p className="text-sm text-muted-foreground">{d.described.plot}</p>}
            </section>
          )}

          {d.albums.length > 0 && (
            <section>
              <h3 className="mb-1.5 text-xs text-muted-foreground">{t('In albums')}</h3>
              <ul className="flex flex-wrap gap-1.5">
                {d.albums.map((a) => <li key={a.id}><Link to={`/albums/${a.id}`} className="rounded-full border px-2.5 py-0.5 text-xs hover:bg-accent">{a.name}</Link></li>)}
              </ul>
            </section>
          )}

          {exif.length > 0 && (
            <section>
              <h3 className="mb-1.5 text-xs text-muted-foreground">{t('Camera')}</h3>
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
