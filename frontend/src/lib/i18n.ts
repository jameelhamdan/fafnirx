import { createContext, useContext } from 'react'
import ar from '../../translations/ar.json'
import en from '../../translations/en.json'
import es from '../../translations/es.json'
import ja from '../../translations/ja.json'
import ru from '../../translations/ru.json'
import zh from '../../translations/zh.json'

/**
 * A very small translation service. The English text is the key: write `t('Add mirror')` and the strings of
 * every language live in translations/<lang>.json. `npm run i18n` collects the t('…') calls of src/ into
 * translations/en.json and lists what the other languages still lack. A string without a translation shows in English.
 * Placeholders are written {name} and filled from the second argument: t('Mirror {name} added', { name }).
 */
export const LANGUAGES = {
  en: { label: 'English', dir: 'ltr', strings: en as Record<string, string> },
  es: { label: 'Español', dir: 'ltr', strings: es as Record<string, string> },
  zh: { label: '中文', dir: 'ltr', strings: zh as Record<string, string> },
  ar: { label: 'العربية', dir: 'rtl', strings: ar as Record<string, string> },
  ru: { label: 'Русский', dir: 'ltr', strings: ru as Record<string, string> },
  ja: { label: '日本語', dir: 'ltr', strings: ja as Record<string, string> },
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

/** The English strings with placeholders, as patterns that find them in a message the master wrote. Made when first needed. */
let templates: { pattern: RegExp; names: string[]; key: string }[] | null = null

/**
 * A message of the master (an error) in the current language. The master writes English; its messages are keys of
 * translations/en.json (`npm run i18n` collects them from crates/master), and one made with `format!` is found by its
 * template: "There is no user named bob" is `There is no user named {user}` with `user` = bob. Anything else stays as it is.
 */
export function translateServer(message: string): string {
  if (message in LANGUAGES.en.strings) return tr(message)
  templates ??= Object.keys(LANGUAGES.en.strings)
    .filter((key) => /\{\w+\}/.test(key))
    .map((key) => {
      const names: string[] = []
      const source = key
        .split(/(\{\w+\})/)
        .map((part) => {
          const name = /^\{(\w+)\}$/.exec(part)?.[1]
          if (!name) return part.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
          names.push(name)
          return '(.+?)'
        })
        .join('')
      return { pattern: new RegExp(`^${source}$`, 's'), names, key }
    })
    // The most fixed text first: "Only 5 of 10 bytes arrived" is `Only {received} of {size} bytes arrived`, not `{n} of {total}`.
    .sort((a, b) => b.key.replace(/\{\w+\}/g, '').length - a.key.replace(/\{\w+\}/g, '').length)
  for (const { pattern, names, key } of templates) {
    const found = pattern.exec(message)
    if (found) return tr(key, Object.fromEntries(names.map((name, i) => [name, found[i + 1]])))
  }
  return message
}

export interface I18n {
  lang: Lang
  setLang: (lang: Lang) => void
  /** Counts the times strings were added after the first draw (a module's translations), so that what is shown is drawn again. */
  revision: number
  t: T
}

export const Context = createContext<I18n>({ lang: 'en', setLang: () => {}, revision: 0, t: (text, vars) => translate('en', text, vars) })

/** The `t` function of the current language, and the language itself. */
export function useI18n() {
  return useContext(Context)
}

export function useT(): T {
  return useContext(Context).t
}

/**
 * Adds the strings a module brings (`FrontendModule.strings`: language -> English text -> translation). The app's own strings
 * stay as they are: a module can add texts but not change the ones the app uses. Languages the app does not have are ignored.
 */
export function addStrings(strings: Record<string, Record<string, string>>) {
  let added = false
  for (const [lang, entries] of Object.entries(strings)) {
    if (!isLang(lang)) continue
    const own = LANGUAGES[lang].strings
    for (const [text, translated] of Object.entries(entries)) {
      if (typeof translated === 'string' && !(text in own)) {
        own[text] = translated
        added = true
      }
    }
  }
  if (added) stringListeners.forEach((listen) => listen())
}

const stringListeners = new Set<() => void>()

/** Calls `listen` when strings were added (a module's translations arrived), so that what is shown can be translated again. */
export function onStringsAdded(listen: () => void) {
  stringListeners.add(listen)
  return () => { stringListeners.delete(listen) }
}

/** Marks a string in data (outside a component) for translation; show it with `t(text)`. */
export const msg = (text: string) => text
