import { createContext, useContext } from 'react'
import ar from '../../translations/ar.json'
import en from '../../translations/en.json'

/**
 * A very small translation service. The English text is the key: write `t('Add mirror')` and the strings of
 * every language live in translations/<lang>.json. `npm run i18n` collects the t('…') calls of src/ into
 * translations/en.json and lists what ar.json still lacks. A string without a translation shows in English.
 * Placeholders are written {name} and filled from the second argument: t('Mirror {name} added', { name }).
 */
export const LANGUAGES = {
  en: { label: 'English', dir: 'ltr', strings: en as Record<string, string> },
  ar: { label: 'العربية', dir: 'rtl', strings: ar as Record<string, string> },
} as const

export type Lang = keyof typeof LANGUAGES
export type Vars = Record<string, string | number>
export type T = (text: string, vars?: Vars) => string

export const STORAGE_KEY = 'fafnirx.lang'

/** The language in use, for code that runs outside React (lib/format.ts). Components re-render when it changes. */
let active: Lang = 'en'

/** Called by the provider when the language changes. */
export const setActiveLanguage = (lang: Lang) => { active = lang }

const isLang = (v: string | null | undefined): v is Lang => !!v && v in LANGUAGES

/** The saved choice, else the first browser language we have, else English. */
export function pickLanguage(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (isLang(saved)) return saved
  } catch { /* storage can be blocked */ }
  for (const tag of navigator.languages ?? [navigator.language]) {
    const base = tag.toLowerCase().split('-')[0]
    if (isLang(base)) return base
  }
  return 'en'
}

/** BCP 47 tag for dates and numbers in the current language. */
export const locale = () => (active === 'en' ? 'en-GB' : active)

/** translate() in the current language, for code outside components. */
export const tr: T = (text, vars) => translate(active, text, vars)

export function translate(lang: Lang, text: string, vars?: Vars): string {
  const found = LANGUAGES[lang].strings[text] || text
  return vars ? found.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : found
}

export interface I18n {
  lang: Lang
  setLang: (lang: Lang) => void
  t: T
}

export const Context = createContext<I18n>({ lang: 'en', setLang: () => {}, t: (text, vars) => translate('en', text, vars) })

/** The `t` function of the current language, and the language itself. */
export function useI18n() {
  return useContext(Context)
}

export function useT(): T {
  return useContext(Context).t
}

/** Marks a string in data (outside a component) for translation; show it with `t(text)`. */
export const msg = (text: string) => text
