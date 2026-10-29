/**
 * What a module of the web app may use of the app, and how it joins it: see `registry.ts`. A module imports from here, from
 * `@/components/ui` and from its own folder, and from nothing else of the app (the lint rules check it).
 */
export { toast } from 'sonner'
export { ago } from '@/lib/format'
export { msg, useI18n, useT } from '@/lib/i18n'
export { cn } from '@/lib/utils'
export { useNow } from '@/hooks/use-now'
export { ModuleViewer } from './module-viewer'
export { registerModule, useHasViewer, useQueueSections } from './registry'
export type { FrontendModule, LoadedModule, ModuleFile, ViewerProps } from './registry'
