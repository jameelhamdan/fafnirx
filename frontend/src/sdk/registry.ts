/**
 * What a module of the web app may use of the app, and how it joins it. A module is a folder of `src/modules/<id>/` built on its
 * own (`npm run build:module -- <id>`) into a script whose default export is a {@link FrontendModule}. The module's service
 * serves that script, and the app loads it while the master says the module of the same id is up (`Session.modules`, see
 * `lib/module-loader.ts`). So without the module's service its part of the web app is simply not there, and not even
 * downloaded.
 *
 * A module imports from here, from `@/components/ui` and from its own folder, and from nothing else of the app (the lint
 * rules check it), so that no module needs another. At run time those imports are the app's own copies (`runtime.ts`), so a
 * module shares React, the language and the look of the app instead of bringing its own.
 */
import { useMemo, useSyncExternalStore, type ComponentType } from 'react'
import { useSession } from '@/lib/session'

/** A file as the app shows it to a module: the same for a photo of the library and a file of the file browser. */
export interface ModuleFile {
  /** Its id, which is also its address for the module (`/api/m/<module>/<id>/…`). */
  id: string
  name: string
  /** `image` or `video` for what the library knows, else null. */
  kind: string | null
  /** The file as it is, in place: always the way out when the module cannot show it. */
  src: string
  /** A picture that stands for the file, when there is one. */
  poster?: string
  /** A deleted file is only kept for restoring; a module should not work on it. */
  deleted?: boolean
}

export interface ViewerProps {
  file: ModuleFile
  /** The module cannot show the file; the app offers the download instead. */
  onFail: () => void
  /** Keys work anywhere on the page (for a full-page viewer), not only while focus is in the viewer. */
  globalKeys?: boolean
  className?: string
}

export interface FrontendModule {
  /** Shows files in the viewer and in the details of a file. The first module whose `match` says yes is used. */
  viewer?: { match: (file: ModuleFile) => boolean; component: ComponentType<ViewerProps> }
  /** A part of the queue page, given what the module reports (its `GET /status`). */
  queue?: ComponentType<{ report: unknown }>
}

/** A module the app has loaded; its id is the one the master has it under. */
export interface LoadedModule extends FrontendModule {
  id: string
}

const registered = new Map<string, LoadedModule>()
let snapshot: LoadedModule[] = []
const listeners = new Set<() => void>()

/** Called by the loader when a module's script has arrived. */
export function registerModule(module: LoadedModule) {
  registered.set(module.id, module)
  snapshot = [...registered.values()]
  listeners.forEach((listen) => listen())
}

function subscribe(listen: () => void) {
  listeners.add(listen)
  return () => listeners.delete(listen)
}

/** The modules that are loaded and that the master has up. */
function useActive(): LoadedModule[] {
  const loaded = useSyncExternalStore(subscribe, () => snapshot)
  const up = useSession()?.modules
  return useMemo(() => loaded.filter((m) => up?.[m.id]), [loaded, up])
}

export const viewerOf = (modules: LoadedModule[], file: ModuleFile) => modules.find((m) => m.viewer?.match(file))?.viewer?.component

/** The component that shows this file, if a module that is up and loaded can. */
export function useViewer(file: ModuleFile) {
  return viewerOf(useActive(), file)
}

/** Whether a module that is up can show this file. */
export function useHasViewer(file: ModuleFile): boolean {
  return useViewer(file) !== undefined
}

/** The parts of the queue page, with the report each was given. */
export function useQueueSections(reports: Record<string, unknown>) {
  return useActive().flatMap((m) => {
    const Section = m.queue
    return Section && reports[m.id] !== undefined ? [{ id: m.id, Section, report: reports[m.id] }] : []
  })
}
