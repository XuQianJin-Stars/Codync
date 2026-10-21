import phrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/zh-Hans.json' with { type: 'json' }

/** This app's interface language. Chats and bot names are not translated. */
export type AppLanguage = 'system' | 'zh' | 'en'

const table = phrases as Record<string, string>

export function systemIsChinese(language = typeof navigator === 'undefined' ? '' : navigator.language): boolean {
  return language.toLowerCase().startsWith('zh')
}

export function usesChinese(preference: AppLanguage, systemChinese: boolean): boolean {
  if (preference === 'zh') return true
  if (preference === 'en') return false
  return systemChinese
}

/** Exact chrome phrases, including a label whose JSX kept the surrounding spaces. */
export function translate(text: string, preference: AppLanguage, systemChinese: boolean): string {
  if (!usesChinese(preference, systemChinese)) return text
  const direct = table[text]
  if (direct) return direct
  const trimmed = text.trim()
  return trimmed === text ? text : (table[trimmed] ?? text)
}

/** "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置". */
export function resetLine(raw: string, preference: AppLanguage, systemChinese: boolean): string {
  if (!usesChinese(preference, systemChinese)) return raw
  const time = raw.startsWith('resets in ') ? raw.slice('resets in '.length) : raw.startsWith('resets ') ? raw.slice('resets '.length) : null
  if (time == null) return translate(raw, preference, systemChinese)
  const spoken = compactTime(time, (key) => translate(key, preference, systemChinese))
  return spoken ? spoken + translate('after reset', preference, systemChinese) : `${translate('resets', preference, systemChinese)} ${time}`
}

function compactTime(time: string, tr: (key: string) => string): string | null {
  let spoken = ''
  let matched = false
  for (const token of time.split(' ')) {
    const unit = token.at(-1)
    const n = Number(token.slice(0, -1))
    if (!unit || token.length < 2 || !Number.isInteger(n)) return null
    const word = unit === 'd' ? tr('day unit') : unit === 'h' ? tr('hour unit') : unit === 'm' ? tr('minute unit') : null
    if (!word) return null
    matched = true
    if (n === 0) continue
    spoken += `${n}${word}`
  }
  if (!matched) return null
  return spoken || `0${tr('minute unit')}`
}

export function languageTitle(language: AppLanguage, preference: AppLanguage, systemChinese: boolean): string {
  if (language === 'zh') return '中文'
  if (language === 'en') return 'English'
  return translate('Follow system', preference, systemChinese)
}
