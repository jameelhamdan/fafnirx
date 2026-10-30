import { useCallback, useEffect, useRef, useState } from 'react'

/**
 * Pictures (thumbnails and previews) are made when somebody looks at a file: the first request for one is answered with
 * "accepted" (202) and queues a job, and the master says over the event stream when the picture is there. Until then the
 * place of the picture stays empty, and the request is made again when the master says so.
 */
const listeners = new Map<string, Set<() => void>>()

/** The master says that a picture of the file with this id is there, or that there will not be one. */
export function pictureReady(id: string) {
  listeners.get(id)?.forEach((again) => again())
}

function listen(id: string | null, again: () => void) {
  if (!id) return () => {}
  const set = listeners.get(id) ?? new Set()
  set.add(again)
  listeners.set(id, set)
  return () => {
    set.delete(again)
    if (set.size === 0) listeners.delete(id)
  }
}

/** The id of a library file in the address of one of its pictures. */
const idOf = (url: string) => /^\/api\/media\/([^/]+)\//.exec(url)?.[1] ?? null

/** In case the news was missed (the event stream was down for a moment), a waiting picture is asked for now and then. */
const RECHECK_MS = 15_000

export type PictureState = 'ok' | 'pending' | 'none'

/**
 * What an `<img>` that shows a picture needs: give it `key={attempt}`, `onLoad` and `onError`. While `state` is `pending` the
 * picture is being made; `none` means there will not be one (thumbnails are off, or the file cannot be drawn).
 */
export function usePicture(url: string) {
  const [wait, setWait] = useState<{ url: string; state: 'pending' | 'none' } | null>(null)
  const [attempt, setAttempt] = useState(0)
  const misses = useRef(0)
  const state: PictureState = wait?.url === url ? wait.state : 'ok'

  useEffect(() => {
    if (state !== 'pending') return
    const again = () => setAttempt((n) => n + 1)
    const off = listen(idOf(url), again)
    const timer = window.setInterval(again, RECHECK_MS)
    return () => {
      off()
      window.clearInterval(timer)
    }
  }, [state, url])

  const onError = useCallback(() => {
    // Why it did not load: a picture on its way answers 202, one that will not come 404.
    fetch(url).then(
      (r) => {
        if (r.status === 202) setWait({ url, state: 'pending' })
        else if (r.ok && misses.current++ < 2) setAttempt((n) => n + 1) // it was made in between
        else setWait({ url, state: 'none' })
      },
      () => setWait({ url, state: 'none' }),
    )
  }, [url])
  const onLoad = useCallback(() => {
    misses.current = 0
    setWait(null)
  }, [])
  return { attempt, state, onError, onLoad }
}

/** The address of a picture once it can be shown (for a video's poster, which is not an `<img>`), else nothing. */
export function useReadyUrl(url?: string): string | undefined {
  const [ready, setReady] = useState<string | undefined>()
  useEffect(() => {
    if (!url) return
    let alive = true
    let off = () => {}
    let timer = 0
    const stop = () => {
      off()
      window.clearInterval(timer)
      timer = 0
    }
    const probe = () =>
      fetch(url).then(
        (r) => {
          if (!alive) return
          if (r.ok) {
            setReady(url)
            stop()
          } else if (r.status === 202) {
            if (timer === 0) {
              off = listen(idOf(url), () => void probe())
              timer = window.setInterval(() => void probe(), RECHECK_MS)
            }
          } else {
            stop()
          }
        },
        () => {},
      )
    void probe()
    return () => {
      alive = false
      stop()
    }
  }, [url])
  return ready === url ? url : undefined
}
