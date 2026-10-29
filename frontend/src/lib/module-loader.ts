import { useEffect } from 'react'
import { registerModule, type FrontendModule } from '@/sdk'
import { publishRuntime, SDK } from '@/sdk/runtime'
import { useSession } from './session'
import type { ModuleInfo } from './types'

const started = new Set<string>()

/** Where a module's files are, through the master (which checks the login). */
const base = (id: string) => `/api/m/${id}/ui/`

async function load(id: string, info: ModuleInfo) {
  const ui = info.ui
  if (!ui) return
  const key = `${id}@${info.version}`
  if (started.has(key)) return
  started.add(key)
  if (ui.sdk !== SDK) {
    console.warn(`The ${id} module was built for another version of the app (${ui.sdk}, this is ${SDK}) and is not loaded.`)
    return
  }
  // The version in the address makes a new version of a module come in place of the one the browser kept.
  const version = encodeURIComponent(info.version)
  try {
    const script: { default: FrontendModule } = await import(/* @vite-ignore */ `${base(id)}${ui.entry}?v=${version}`)
    if (ui.css) {
      const link = document.createElement('link')
      link.rel = 'stylesheet'
      link.href = `${base(id)}${ui.css}?v=${version}`
      document.head.prepend(link) // before the app's own style sheet: see modules/video/style.css
    }
    registerModule({ ...script.default, id })
  } catch (e) {
    started.delete(key)
    console.warn(`The ${id} module's part of the app could not be loaded:`, e)
  }
}

/** Loads the script of every module the master has up, once. Until it has arrived the module's files open as plain downloads. */
export function useModuleScripts() {
  const modules = useSession()?.modules
  useEffect(() => {
    if (!modules) return
    publishRuntime()
    for (const [id, info] of Object.entries(modules)) void load(id, info)
  }, [modules])
}
