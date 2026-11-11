import { createContext, useContext, useEffect } from 'react'
import { useLocation, useNavigate, type Location } from 'react-router-dom'
import type { Asset } from '@/lib/types'

/**
 * Opening a photo or video puts its own address, `/watch/<id>`, in the address bar while the page it was opened from stays
 * under it: the address can be sent to whoever may see the file, closing goes back to the page, and the page keeps its place.
 * The page says which files the viewer steps through; with none (the address was opened on its own, or after a reload) the
 * viewer steps through the folder of the file.
 */
export interface ViewerSource {
  assets: Asset[]
  /** The viewer reached the first or last file: the page may load more. */
  onEdge?: (direction: 'prev' | 'next') => void
}

export interface Holder {
  source: ViewerSource | null
  set: (source: ViewerSource | null) => void
}

/** Held by `ViewerSourceProvider` (components/viewer-source-provider). */
export const Context = createContext<Holder>({ source: null, set: () => {} })

/** The files the open page lets the viewer step through, if any. */
export const useCurrentSource = () => useContext(Context).source

/** A page tells the viewer what it shows, for as long as it is on screen. */
export function useViewerSource(source: ViewerSource | null) {
  const { set } = useContext(Context)
  useEffect(() => {
    set(source)
    return () => set(null)
  }, [source, set])
}

/** The page under an open viewer, kept in the state of the address that opened it. */
export const backgroundOf = (location: Location): Location | undefined =>
  (location.state as { background?: Location } | null)?.background

/** Opens the viewer of a file over the current page. */
export function useOpenMedia() {
  const navigate = useNavigate()
  const location = useLocation()
  return (id: string) => navigate(`/watch/${id}`, { state: { background: location } })
}

/** The state of a link to a viewer that opens over the current page. */
export function useWatchState() {
  const location = useLocation()
  return { background: location }
}
