import { Check, Languages } from 'lucide-react'
import { LANGUAGES, useI18n, type Lang } from '@/lib/i18n'
import { Button } from './ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from './ui/dropdown-menu'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

/** Picks the language of the web app. Sits next to the theme button. */
export function LanguageMenu() {
  const { lang, setLang, t } = useI18n()
  const label = t('Language: {language}', { language: LANGUAGES[lang].label })
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon" aria-label={label}>
              <Languages className="size-4" />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        {(Object.keys(LANGUAGES) as Lang[]).map((code) => (
          <DropdownMenuItem key={code} lang={code} onSelect={() => setLang(code)}>
            <Check className={code === lang ? 'size-4' : 'size-4 opacity-0'} />
            {LANGUAGES[code].label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
