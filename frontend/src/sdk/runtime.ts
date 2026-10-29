/**
 * What the app hands to the modules it loads at run time. A module is built apart from the app (`vite.module.config.ts`), with
 * these imports left out: `react`, `react/jsx-runtime`, `@/sdk` and `@/components/ui/*` are read from `window.__fafnirx` instead.
 * So a module has the app's own React (hooks need exactly one), language, toasts and look, and weighs only what is its own.
 */
import * as React from 'react'
import * as jsx from 'react/jsx-runtime'
import * as sdk from './index'

/** The version of this contract. It is the number `fafnirx_module_api::WEB_SDK` has (a test checks it): a module's script that
 * was built for another one is not loaded. Change both when what a module may import changes in a way that breaks it. */
export const SDK = 1

declare global {
  interface Window {
    __fafnirx?: { sdk: number; shared: Record<string, unknown> }
  }
}

const ui = import.meta.glob('../components/ui/*.tsx', { eager: true })

/** Makes the shared modules available to the scripts that will be loaded. Called once, before any of them. */
export function publishRuntime() {
  const shared: Record<string, unknown> = { react: React, 'react/jsx-runtime': jsx, '@/sdk': sdk }
  for (const [path, module] of Object.entries(ui)) shared[`@/components/ui/${path.split('/').pop()!.replace(/\.tsx$/, '')}`] = module
  window.__fafnirx = { sdk: SDK, shared }
}
