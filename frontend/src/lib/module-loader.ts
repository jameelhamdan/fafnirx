import { useEffect } from 'react'
import { registerModule, type FrontendModule } from '@/sdk'
import { addStrings, useI18n, type Lang } from './i18n'
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

const stringsLoaded = new Set<string>()

/** Merges the translations a module brings for the language in use (`strings.<lang>.json`, English text -> translation), once per language. */
async function loadStrings(id: string, info: ModuleInfo, lang: Lang) {
  const key = `${id}@${info.version}:${lang}`
  if (lang === 'en' || !info.ui?.strings?.includes(lang) || stringsLoaded.has(key)) return
  stringsLoaded.add(key)
  try {
    const found = await fetch(`${base(id)}strings.${lang}.json?v=${encodeURIComponent(info.version)}`)
    if (!found.ok) throw new Error(String(found.status))
    addStrings({ [lang]: (await found.json()) as Record<string, string> })
  } catch (e) {
    stringsLoaded.delete(key)
    console.warn(`The ${id} module's translations could not be loaded:`, e)
  }
}

/** Loads the script of every module the master has up, once. Until it has arrived the module's files open as plain downloads. */
export function useModuleScripts() {
  const modules = useSession()?.modules
  const { lang } = useI18n()
  useEffect(() => {
    if (!modules) return
    publishRuntime()
    for (const [id, info] of Object.entries(modules)) void load(id, info)
  }, [modules])
  // Also when the language changes: the words of a module arrive for the language in use.
  useEffect(() => {
    if (!modules) return
    for (const [id, info] of Object.entries(modules)) void loadStrings(id, info, lang)
  }, [modules, lang])
}
