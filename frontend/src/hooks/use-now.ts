import { useEffect, useState } from 'react'

/** The current time in unix seconds, refreshed every `every` seconds, for "3 min ago" labels. */
export function useNow(every = 15): number {
  const [now, setNow] = useState(() => Date.now() / 1000)
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now() / 1000), every * 1000)
    return () => window.clearInterval(t)
  }, [every])
  return now
}
