import { createElement, Suspense, type ReactNode } from 'react'
import { useViewer, type ViewerProps } from './registry'

/** Shows the file with the module that handles it (`fallback` when none that is up does, `loading` while its code arrives). */
export function ModuleViewer({ fallback, loading, ...props }: ViewerProps & { fallback?: ReactNode; loading?: ReactNode }) {
  const viewer = useViewer(props.file)
  if (!viewer) return fallback
  return <Suspense fallback={loading}>{createElement(viewer, props)}</Suspense>
}
