// Chat translation, from the server unity-explorer uses: autotranslate-server, a LibreTranslate.
// A message goes as its text runs only, with links, coordinates, worlds, @mentions and emoji
// kept back (the server drops emoji and whatever follows them in a run), all runs in one batch
// request; the answers go back between the parts kept. Translations live for the session only:
// nothing here is stored with the DM history. A failed request just leaves the original.

import { useSyncExternalStore } from 'react'
import { BASE_DOMAIN } from '../../lib/baseDomain'
import { PREF, getPref, setPref } from '../../lib/prefs'
import { parseMessage } from './chatText'

/** The server's target languages (GET /languages), with each one's name in itself (CLDR). Kept
 *  here rather than asked of Intl.DisplayNames: Chromium's trimmed locale data lacks some. */
export const LANGUAGES: readonly { code: string; name: string; native: string }[] = [
  { code: 'sq', name: 'Albanian', native: 'Shqip' },
  { code: 'ar', name: 'Arabic', native: 'العربية' },
  { code: 'az', name: 'Azerbaijani', native: 'Azərbaycan' },
  { code: 'eu', name: 'Basque', native: 'Euskara' },
  { code: 'bn', name: 'Bengali', native: 'বাংলা' },
  { code: 'bg', name: 'Bulgarian', native: 'Български' },
  { code: 'ca', name: 'Catalan', native: 'Català' },
  { code: 'zh-Hans', name: 'Chinese (Simplified)', native: '简体中文' },
  { code: 'zh-Hant', name: 'Chinese (Traditional)', native: '繁體中文' },
  { code: 'cs', name: 'Czech', native: 'Čeština' },
  { code: 'da', name: 'Danish', native: 'Dansk' },
  { code: 'nl', name: 'Dutch', native: 'Nederlands' },
  { code: 'en', name: 'English', native: 'English' },
  { code: 'eo', name: 'Esperanto', native: 'Esperanto' },
  { code: 'et', name: 'Estonian', native: 'Eesti' },
  { code: 'fi', name: 'Finnish', native: 'Suomi' },
  { code: 'fr', name: 'French', native: 'Français' },
  { code: 'gl', name: 'Galician', native: 'Galego' },
  { code: 'de', name: 'German', native: 'Deutsch' },
  { code: 'el', name: 'Greek', native: 'Ελληνικά' },
  { code: 'he', name: 'Hebrew', native: 'עברית' },
  { code: 'hi', name: 'Hindi', native: 'हिन्दी' },
  { code: 'hu', name: 'Hungarian', native: 'Magyar' },
  { code: 'id', name: 'Indonesian', native: 'Indonesia' },
  { code: 'ga', name: 'Irish', native: 'Gaeilge' },
  { code: 'it', name: 'Italian', native: 'Italiano' },
  { code: 'ja', name: 'Japanese', native: '日本語' },
  { code: 'ko', name: 'Korean', native: '한국어' },
  { code: 'ky', name: 'Kyrgyz', native: 'Кыргызча' },
  { code: 'lv', name: 'Latvian', native: 'Latviešu' },
  { code: 'lt', name: 'Lithuanian', native: 'Lietuvių' },
  { code: 'ms', name: 'Malay', native: 'Melayu' },
  { code: 'nb', name: 'Norwegian', native: 'Norsk bokmål' },
  { code: 'fa', name: 'Persian', native: 'فارسی' },
  { code: 'pl', name: 'Polish', native: 'Polski' },
  { code: 'pt', name: 'Portuguese', native: 'Português' },
  { code: 'pt-BR', name: 'Portuguese (Brazil)', native: 'Português (Brasil)' },
  { code: 'ro', name: 'Romanian', native: 'Română' },
  { code: 'ru', name: 'Russian', native: 'Русский' },
  { code: 'sk', name: 'Slovak', native: 'Slovenčina' },
  { code: 'sl', name: 'Slovenian', native: 'Slovenščina' },
  { code: 'es', name: 'Spanish', native: 'Español' },
  { code: 'sv', name: 'Swedish', native: 'Svenska' },
  { code: 'tl', name: 'Tagalog', native: 'Filipino' },
  { code: 'th', name: 'Thai', native: 'ไทย' },
  { code: 'tr', name: 'Turkish', native: 'Türkçe' },
  { code: 'uk', name: 'Ukrainian', native: 'Українська' },
  { code: 'ur', name: 'Urdu', native: 'اردو' }
]

/** The server language for a BCP 47 tag ("pt-BR", "zh-TW", "de-AT"), or null if it has none. */
export function matchLanguage(tag: string): string | null {
  const t = tag.trim().toLowerCase()
  const exact = LANGUAGES.find((l) => l.code.toLowerCase() === t)
  if (exact != null) return exact.code
  const [base, ...rest] = t.split(/[-_]/)
  if (base === 'zh') return rest.some((r) => ['hant', 'tw', 'hk', 'mo'].includes(r)) ? 'zh-Hant' : 'zh-Hans'
  const alias: Record<string, string> = { no: 'nb', nn: 'nb', fil: 'tl', iw: 'he', in: 'id' }
  const code = alias[base] ?? base
  return LANGUAGES.find((l) => l.code === code)?.code ?? null
}

/** The first of the browser's languages the server has, else English. */
export function systemLanguage(tags: readonly string[] = navigator.languages ?? [navigator.language]): string {
  for (const tag of tags) {
    const code = matchLanguage(tag)
    if (code != null) return code
  }
  return 'en'
}

/** "Español" for "es": the language's name in itself, for choosing one. */
export function nativeName(code: string): string {
  return LANGUAGES.find((l) => l.code === code)?.native ?? languageName(code)
}

/** "Spanish" for "es"; the code itself for one we have no name for. */
export function languageName(code: string): string {
  const match = LANGUAGES.find((l) => l.code === code) ?? LANGUAGES.find((l) => l.code === code.split('-')[0])
  if (match != null) return match.name
  try {
    return new Intl.DisplayNames(['en'], { type: 'language' }).of(code) ?? code
  } catch {
    return code
  }
}

const EMOJI_RUN = /[\p{Extended_Pictographic}\p{Regional_Indicator}\p{Emoji_Modifier}‍️⃣]+/gu
const LETTER = /\p{L}/u

/** A message cut into the runs sent for translation and the parts kept as they are. */
export function splitForTranslation(text: string): { value: string; translate: boolean }[] {
  const parts: { value: string; translate: boolean }[] = []
  for (const token of parseMessage(text)) {
    if (token.type !== 'text') {
      parts.push({ value: token.value, translate: false })
      continue
    }
    let last = 0
    for (const m of token.value.matchAll(EMOJI_RUN)) {
      const i = m.index ?? 0
      if (i > last) parts.push({ value: token.value.slice(last, i), translate: true })
      parts.push({ value: m[0], translate: false })
      last = i + m[0].length
    }
    if (last < token.value.length) parts.push({ value: token.value.slice(last), translate: true })
  }
  // runs without a letter (numbers, punctuation, spaces) need no translating
  return parts.map((p) => (p.translate && !LETTER.test(p.value) ? { ...p, translate: false } : p))
}

/** Does a message have anything to translate? */
export function hasTranslatableText(text: string): boolean {
  return splitForTranslation(text).some((p) => p.translate)
}

export const TRANSLATE_URL = `https://autotranslate-server.${BASE_DOMAIN}/translate`
const TIMEOUT_MS = 20_000

interface BatchResponse {
  translatedText: string[]
  detectedLanguage?: { language: string }[]
}

/**
 * `text` in `target`, with the language it was detected in; null when there was nothing to
 * translate or it is already in `target`. Throws on a failed or malformed answer.
 */
export async function translateText(text: string, target: string): Promise<{ text: string; from: string } | null> {
  const parts = splitForTranslation(text)
  // each run goes without its outer spaces, which are put back around its translation
  const runs = parts.flatMap((p, i) => {
    if (!p.translate) return []
    const lead = /^\s*/.exec(p.value)?.[0] ?? ''
    const trail = /\s*$/.exec(p.value)?.[0] ?? ''
    return [{ i, lead, trail, core: p.value.slice(lead.length, p.value.length - trail.length) }]
  })
  if (runs.length === 0) return null

  const abort = new AbortController()
  const timer = setTimeout(() => abort.abort(), TIMEOUT_MS)
  let body: BatchResponse
  try {
    const res = await fetch(TRANSLATE_URL, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ q: runs.map((r) => r.core), source: 'auto', target, format: 'text' }),
      signal: abort.signal
    })
    if (!res.ok) throw new Error(`translate: HTTP ${res.status}`)
    body = (await res.json()) as BatchResponse
  } finally {
    clearTimeout(timer)
  }
  const out = body.translatedText
  if (!Array.isArray(out) || out.length !== runs.length || out.some((t) => typeof t !== 'string')) {
    throw new Error('translate: malformed answer')
  }

  // the language of most of the text
  const weight = new Map<string, number>()
  runs.forEach((r, k) => {
    const lang = body.detectedLanguage?.[k]?.language
    if (lang) weight.set(lang, (weight.get(lang) ?? 0) + r.core.length)
  })
  const from = [...weight.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? ''
  if (from !== '' && from.split('-')[0] === target.split('-')[0]) return null

  const values = parts.map((p) => p.value)
  runs.forEach((r, k) => {
    values[r.i] = r.lead + out[k] + r.trail
  })
  const translated = values.join('')
  return translated === text ? null : { text: translated, from }
}

// ── Settings ────────────────────────────────────────────────────────────────

export interface TranslationPrefs {
  /** The server code messages are translated into. */
  language: string
  /** Whether the language was chosen, rather than taken from the system. */
  languageChosen: boolean
  /** The auto-translate state of a conversation not switched on or off by hand. */
  autoDefault: boolean
  /** Conversations switched on or off by hand ('Nearby' or a lowercase wallet). */
  channels: Readonly<Record<string, boolean>>
}

let prefs: TranslationPrefs | null = null
const listeners = new Set<() => void>()

function readPrefs(): TranslationPrefs {
  const chosen = getPref(PREF.chatTranslateLanguage)
  const language = chosen != null ? matchLanguage(chosen) : null
  let channels: Record<string, boolean> = {}
  try {
    const parsed: unknown = JSON.parse(getPref(PREF.chatAutoTranslateChannels) ?? '{}')
    if (parsed != null && typeof parsed === 'object') {
      channels = Object.fromEntries(Object.entries(parsed).filter((e): e is [string, boolean] => typeof e[1] === 'boolean'))
    }
  } catch {}
  return {
    language: language ?? systemLanguage(),
    languageChosen: language != null,
    autoDefault: getPref(PREF.chatAutoTranslateDefault) === 'true',
    channels
  }
}

function getPrefs(): TranslationPrefs {
  // read on first use: the stored values are only loaded when the HUD is about to render
  prefs ??= readPrefs()
  return prefs
}

function updatePrefs(next: TranslationPrefs): void {
  prefs = next
  listeners.forEach((l) => l())
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function useTranslationPrefs(): TranslationPrefs {
  return useSyncExternalStore(subscribe, getPrefs)
}

/** Choose the language messages are translated into; null goes back to the system's. */
export function setTranslationLanguage(code: string | null): void {
  setPref(PREF.chatTranslateLanguage, code ?? '')
  updatePrefs({ ...getPrefs(), language: code ?? systemLanguage(), languageChosen: code != null })
}

export function setAutoTranslateDefault(on: boolean): void {
  setPref(PREF.chatAutoTranslateDefault, String(on))
  updatePrefs({ ...getPrefs(), autoDefault: on })
}

export function autoTranslates(p: TranslationPrefs, channel: string): boolean {
  return p.channels[channel] ?? p.autoDefault
}

export function setAutoTranslate(channel: string, on: boolean): void {
  const channels = { ...getPrefs().channels, [channel]: on }
  setPref(PREF.chatAutoTranslateChannels, JSON.stringify(channels))
  updatePrefs({ ...getPrefs(), channels })
}

/** Back to the default for a conversation, or (no channel) for all of them. */
export function clearAutoTranslate(channel?: string): void {
  const channels = channel == null ? {} : Object.fromEntries(Object.entries(getPrefs().channels).filter(([c]) => c !== channel))
  setPref(PREF.chatAutoTranslateChannels, JSON.stringify(channels))
  updatePrefs({ ...getPrefs(), channels })
}

// ── Translations ────────────────────────────────────────────────────────────

export type Translation =
  | { status: 'pending' }
  | { status: 'done'; text: string; from: string; showOriginal: boolean }
  /** Nothing to translate, or already in the language (`to`); `manual`: asked for by hand, so it is shown. */
  | { status: 'same'; manual: boolean; to: string }
  /** `manual`: asked for by hand, so the failure is shown. */
  | { status: 'failed'; manual: boolean }

const MAX_ENTRIES = 500
const MAX_IN_FLIGHT = 4
const MAX_QUEUED = 30
// After this many failures in a row, auto-translation pauses for a while rather than send every
// new message to a server that isn't answering.
const FAILURES_TO_PAUSE = 3
const PAUSE_MS = 60_000

const entries = new Map<string, Translation>()
const entryListeners = new Set<() => void>()
let version = 0
const queue: { key: string; run: () => Promise<void> }[] = []
let inFlight = 0
let failures = 0
let pausedUntil = 0

function entryKey(id: string, target: string): string {
  return `${target}|${id}`
}

function setEntry(key: string, entry: Translation | null): void {
  if (entry == null) entries.delete(key)
  else {
    entries.delete(key)
    entries.set(key, entry)
    // the oldest go first; a pending one is re-requested if it is still wanted
    if (entries.size > MAX_ENTRIES) entries.delete(entries.keys().next().value as string)
  }
  version++
  entryListeners.forEach((l) => l())
}

function pump(): void {
  while (inFlight < MAX_IN_FLIGHT && queue.length > 0) {
    const job = queue.shift()!
    inFlight++
    void job.run().finally(() => {
      inFlight--
      pump()
    })
  }
}

/** A message's translation into `target`, if one was asked for this session. */
export function getTranslation(id: string, target: string): Translation | undefined {
  return entries.get(entryKey(id, target))
}

/** Re-renders when any translation changes; read them with getTranslation. */
export function useTranslations(): number {
  return useSyncExternalStore(
    (l) => {
      entryListeners.add(l)
      return () => entryListeners.delete(l)
    },
    () => version
  )
}

/**
 * Translate a message into `target`. `id` names it across renders. A message already translated
 * or on its way is left alone; one that failed is retried only by hand.
 */
export function requestTranslation(id: string, text: string, target: string, manual: boolean): void {
  const key = entryKey(id, target)
  const existing = entries.get(key)
  if (existing != null && (existing.status !== 'failed' || !manual)) return
  if (!manual && Date.now() < pausedUntil) return
  setEntry(key, { status: 'pending' })
  queue.push({
    key,
    run: async () => {
      try {
        const result = await translateText(text, target)
        failures = 0
        setEntry(key, result == null ? { status: 'same', manual, to: target } : { status: 'done', ...result, showOriginal: false })
      } catch (e) {
        console.warn('[translate]', e)
        if (++failures >= FAILURES_TO_PAUSE) {
          failures = 0
          pausedUntil = Date.now() + PAUSE_MS
        }
        setEntry(key, { status: 'failed', manual })
      }
    }
  })
  // a burst keeps the newest: the oldest waiting go unasked, and can be asked again
  while (queue.length > MAX_QUEUED) setEntry(queue.shift()!.key, null)
  pump()
}

/** Show a translated message in its original words, or its translation again. */
export function showOriginal(id: string, target: string, original: boolean): void {
  const key = entryKey(id, target)
  const entry = entries.get(key)
  if (entry?.status !== 'done' || entry.showOriginal === original) return
  setEntry(key, { ...entry, showOriginal: original })
}

/** Forget everything: for tests. */
export function resetTranslations(): void {
  entries.clear()
  queue.length = 0
  failures = 0
  pausedUntil = 0
  prefs = null
  version++
}
