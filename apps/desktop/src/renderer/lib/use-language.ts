import { useSyncExternalStore } from 'react'
import { prefs } from './prefs'
import { resetLine, systemIsChinese, translate, type AppLanguage } from './l10n'

export function useAppLanguage(): AppLanguage {
  return useSyncExternalStore(prefs.appLanguage.subscribe, prefs.appLanguage.get)
}

/** Translates one chrome string for the language chosen in Settings. */
export function useTr(): (text: string) => string {
  const language = useAppLanguage()
  const systemChinese = systemIsChinese()
  return (text) => translate(text, language, systemChinese)
}

/** "resets 3h 20m" for the language chosen in Settings. */
export function useResetLine(): (raw: string) => string {
  const language = useAppLanguage()
  const systemChinese = systemIsChinese()
  return (raw) => resetLine(raw, language, systemChinese)
}
