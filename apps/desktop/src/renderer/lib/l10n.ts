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

/** One piece of a phrase key such as "Runs the official installer on {name}." */
interface Piece { literal?: string; token?: string }

/**
 * Phrase keys that already contain `{token}` slots, longest fixed text first.
 * A filled sentence ("Runs the official installer on this Mac.") matches the key,
 * and a slot that is itself a known phrase (a setup hint) is translated too.
 */
const templates: Array<{ key: string; pieces: Piece[]; literal: number }> = Object.keys(table).flatMap((key) => {
  const pieces = parseTemplate(key)
  if (!pieces) return []
  const literal = pieces.reduce((n, piece) => n + (piece.literal?.length ?? 0), 0)
  return literal >= 6 ? [{ key, pieces, literal }] : []
}).sort((a, b) => b.literal - a.literal || b.key.length - a.key.length)

function parseTemplate(template: string): Piece[] | undefined {
  const pieces: Piece[] = []
  let rest = template
  while (rest.includes('{')) {
    const open = rest.indexOf('{')
    if (open > 0) pieces.push({ literal: rest.slice(0, open) })
    const close = rest.indexOf('}', open)
    if (close < 0) return undefined
    const token = rest.slice(open, close + 1)
    const name = token.slice(1, -1)
    if (name.length === 0 || !/^[A-Za-z0-9]+$/.test(name) || pieces.at(-1)?.token) return undefined
    pieces.push({ token })
    rest = rest.slice(close + 1)
  }
  if (rest.length > 0) pieces.push({ literal: rest })
  return pieces.some((piece) => piece.token) ? pieces : undefined
}

function fill(text: string, pieces: Piece[], phrase: string, depth: number): string | undefined {
  let pos = 0
  const caps = new Map<string, string>()
  for (let i = 0; i < pieces.length; i++) {
    const piece = pieces[i]
    if (piece.literal !== undefined) {
      if (!text.startsWith(piece.literal, pos)) return undefined
      pos += piece.literal.length
      continue
    }
    const token = piece.token ?? ''
    const next = pieces[i + 1]?.literal
    const end = next === undefined ? text.length : text.indexOf(next, pos)
    if (end < 0 || end === pos) return undefined
    caps.set(token, text.slice(pos, end))
    pos = end
  }
  if (pos !== text.length) return undefined
  let result = phrase
  for (const [token, value] of caps) {
    const inner = table[value] ?? (depth === 0 ? templated(value, 1) : undefined) ?? value
    result = result.replaceAll(token, inner)
  }
  return result
}

/** A sentence the host or a view already filled in, matched back to its `{token}` phrase. */
function templated(text: string, depth = 0): string | undefined {
  for (const template of templates) {
    const phrase = table[template.key]
    const filled = phrase ? fill(text, template.pieces, phrase, depth) : undefined
    if (filled) return filled
  }
  return undefined
}

function phrase(text: string): string | undefined {
  return table[text] ?? templated(text)
}

/** Exact chrome phrases, including a label whose JSX kept the surrounding spaces. */
export function translate(text: string, preference: AppLanguage, systemChinese: boolean): string {
  if (!usesChinese(preference, systemChinese)) return text
  const direct = phrase(text)
  if (direct) return direct
  const trimmed = text.trim()
  return trimmed === text ? text : (phrase(trimmed) ?? text)
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
