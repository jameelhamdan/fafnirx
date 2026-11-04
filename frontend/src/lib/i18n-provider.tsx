import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Context, LANGUAGES, onStringsAdded, pickLanguage, setActiveLanguage, STORAGE_KEY, translate, type I18n, type Lang } from './i18n'

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(() => { const lang = pickLanguage(); setActiveLanguage(lang); return lang })
  useEffect(() => {
    document.documentElement.lang = lang
    document.documentElement.dir = LANGUAGES[lang].dir
  }, [lang])
  const setLang = useCallback((next: Lang) => {
    setActiveLanguage(next)
    setLangState(next)
    try { localStorage.setItem(STORAGE_KEY, next) } catch { /* not saved */ }
  }, [])
  // A module's translations arrive after the app has been drawn: `t` changes, and what uses it is drawn again.
  const [revision, setRevision] = useState(0)
  useEffect(() => onStringsAdded(() => setRevision((n) => n + 1)), [])
  const value = useMemo<I18n>(() => ({ lang, setLang, revision, t: (text, vars) => translate(lang, text, vars) }), [lang, setLang, revision])
  return <Context.Provider value={value}>{children}</Context.Provider>
}
