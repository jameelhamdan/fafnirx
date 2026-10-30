import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { useEffect, useState, type ReactNode } from 'react'
import { LiveContext } from './live-state'
import { pictureReady } from './pictures'

/** What the master announces: a topic and a key (a device name, a table, or empty). */
interface Change {
  topic: string
  key: string
}

/** The query keys a change makes stale. Queries are keyed by their kind first. */
function staleKeys(change: Change): string[][] {
  switch (change.topic) {
    case 'files':
      return [['files'], ['info'], ['dashboard'], ['devices'], ['timeline'], ['asset'], ['trash'], ['albums'], ['album']]
    case 'library':
      return [['timeline'], ['asset'], ['albums'], ['album'], ['tags']]
    case 'activity':
      return [['dashboard'], ['device']]
    case 'devices':
      return [['devices'], ['device'], ['dashboard'], ['files'], ['shared']]
    case 'alerts':
      return [['dashboard']]
    case 'jobs':
      return [['sources'], ['mirrors'], ['dashboard']]
    default:
      return [[]] // resync: everything
  }
}

function refresh(client: QueryClient, pending: Map<string, number>, change: Change) {
  for (const key of staleKeys(change)) {
    const id = key.join('/')
    if (pending.has(id)) continue
    // Many changes arrive in bursts (an upload touches several things); refresh once per burst.
    pending.set(
      id,
      window.setTimeout(() => {
        pending.delete(id)
        void client.invalidateQueries({ queryKey: key })
      }, 400),
    )
  }
}

/** Keeps the pages up to date: listens to the master's event stream and refreshes what changed. */
export function LiveUpdates({ children }: { children: ReactNode }) {
  const client = useQueryClient()
  const [connected, setConnected] = useState(false)

  useEffect(() => {
    const pending = new Map<string, number>()
    const source = new EventSource('/api/events')
    source.onopen = () => setConnected(true)
    source.onerror = () => {
      setConnected(false)
      // The browser reconnects by itself, except after an HTTP error such as an ended login: then check the session.
      if (source.readyState === EventSource.CLOSED) void client.invalidateQueries({ queryKey: ['session'] })
    }
    source.onmessage = (e) => {
      try {
        const change = JSON.parse(e.data) as Change
        // A picture that somebody asked for is made: only what waits for it looks again.
        if (change.topic === 'thumbs') pictureReady(change.key)
        else refresh(client, pending, change)
      } catch {
        /* not one of ours */
      }
    }
    return () => {
      source.close()
      pending.forEach((t) => window.clearTimeout(t))
    }
  }, [client])

  return <LiveContext.Provider value={connected}>{children}</LiveContext.Provider>
}
