import { useMemo, useState, type ReactNode } from 'react'
import { Context, type ViewerSource } from '@/lib/viewer-source'

/** Holds the files the open page lets the viewer step through (lib/viewer-source). */
export function ViewerSourceProvider({ children }: { children: ReactNode }) {
  const [source, set] = useState<ViewerSource | null>(null)
  const value = useMemo(() => ({ source, set }), [source])
  return <Context.Provider value={value}>{children}</Context.Provider>
}
