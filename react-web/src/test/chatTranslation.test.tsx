import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Chat } from '../features/chat/Chat'
import { SettingsPanel } from '../features/settings/SettingsPanel'
import type { ChatLine, ChatState } from '../features/session/useEngineSession'
import {
  TRANSLATE_URL,
  matchLanguage,
  resetTranslations,
  setAutoTranslate,
  setAutoTranslateDefault,
  setTranslationLanguage,
  splitForTranslation,
  systemLanguage,
  translateText
} from '../features/chat/translation'
import { PREF, getPref } from '../lib/prefs'
import { fakeProfileState, fakeSession } from './harness'

/** Answers each translate request with `answer(q)`, recording the requests. */
function mockServer(answer: (q: string[]) => { translatedText: string[]; detectedLanguage: { language: string }[] } | number) {
  const requests: { q: string[]; target: string; source: string; format: string }[] = []
  const realFetch = globalThis.fetch
  const spy = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    if (String(input) !== TRANSLATE_URL) return realFetch(input, init)
    const body = JSON.parse(String(init?.body)) as (typeof requests)[number]
    requests.push(body)
    const a = answer(body.q)
    return Promise.resolve(typeof a === 'number' ? new Response('', { status: a }) : new Response(JSON.stringify(a)))
  })
  return { requests, spy }
}

const es = (map: Record<string, string>) => (q: string[]) => ({
  translatedText: q.map((s) => map[s] ?? s),
  detectedLanguage: q.map(() => ({ language: 'es' }))
})

beforeEach(() => {
  localStorage.clear()
  resetTranslations()
})
afterEach(() => vi.restoreAllMocks())

describe('translation languages', () => {
  it('matches browser tags to the server languages', () => {
    expect(matchLanguage('es-AR')).toBe('es')
    expect(matchLanguage('pt-BR')).toBe('pt-BR')
    expect(matchLanguage('pt-PT')).toBe('pt')
    expect(matchLanguage('zh-TW')).toBe('zh-Hant')
    expect(matchLanguage('zh-CN')).toBe('zh-Hans')
    expect(matchLanguage('no')).toBe('nb')
    expect(matchLanguage('xx')).toBeNull()
  })

  it('the system language is the first the server has, else English', () => {
    expect(systemLanguage(['gsw', 'de-CH', 'en'])).toBe('de')
    expect(systemLanguage(['xx'])).toBe('en')
  })
})

describe('splitting a message for translation', () => {
  it('keeps links, mentions, coordinates and emoji back from the server', () => {
    const parts = splitForTranslation('hola @Ana 😀 ven a 10,20 https://x.org/a ok')
    expect(parts.filter((p) => p.translate).map((p) => p.value)).toEqual(['hola ', ' ', ' ven a ', ' ', ' ok'].filter((v) => /\p{L}/u.test(v)))
    expect(parts.map((p) => p.value).join('')).toBe('hola @Ana 😀 ven a 10,20 https://x.org/a ok')
  })

  it('a message of emoji and numbers has nothing to translate', async () => {
    const { requests } = mockServer(es({}))
    expect(await translateText('👍👍 100', 'en')).toBeNull()
    expect(requests).toHaveLength(0)
  })
})

describe('translateText', () => {
  it('sends the text runs in one batch and puts the answers back between the kept parts', async () => {
    const { requests } = mockServer(es({ hola: 'hello', 'que tal': 'how are you' }))
    expect(await translateText('hola 😀 que tal ', 'en')).toEqual({ text: 'hello 😀 how are you ', from: 'es' })
    expect(requests).toEqual([{ q: ['hola', 'que tal'], source: 'auto', target: 'en', format: 'text' }])
  })

  it('a message already in the language is left as it is', async () => {
    mockServer((q) => ({ translatedText: q, detectedLanguage: q.map(() => ({ language: 'en' })) }))
    expect(await translateText('hello there', 'en')).toBeNull()
  })

  it('throws on an error or a malformed answer', async () => {
    mockServer(() => 500)
    await expect(translateText('hola', 'en')).rejects.toThrow()
    vi.restoreAllMocks()
    mockServer(() => ({ translatedText: [], detectedLanguage: [] }))
    await expect(translateText('hola', 'en')).rejects.toThrow()
  })
})

const me = { address: '0xme', name: 'me' }
const line = (over: Partial<ChatLine>): ChatLine => ({
  sender: '0xana',
  message: 'hola',
  channel: 'Nearby',
  messageId: '0xana:45000.5',
  id: 1,
  ts: 1_700_000_000_000,
  ...over
})
const renderChat = (messages: ChatLine[], channel = 'Nearby') => {
  const conversations = channel === 'Nearby' ? [] : [{ address: channel, unread: 0, state: 'connected' as const, online: true }]
  const chat: ChatState = { ...fakeSession().chat, open: true, channel, messages, conversations }
  return render(<Chat chat={chat} me={me} />)
}

describe('chat translation', () => {
  it('an auto-translating conversation shows the translation, marked, with the original a click away', async () => {
    mockServer(es({ hola: 'hello' }))
    setAutoTranslate('Nearby', true)
    renderChat([line({})])
    expect(await screen.findByText('hello')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'From Spanish · See original' }))
    expect(screen.getByText('hola')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'See translation' }))
    expect(screen.getByText('hello')).toBeInTheDocument()
  })

  it('a translation in flight is an icon of its own; the tooltip follows it when it lands', async () => {
    let release!: () => void
    const gate = new Promise<void>((r) => {
      release = r
    })
    const realFetch = globalThis.fetch
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      if (String(input) !== TRANSLATE_URL) return realFetch(input, init)
      await gate
      return new Response(JSON.stringify(es({ hola: 'hello' })(['hola'])))
    })
    setAutoTranslate('Nearby', true)
    renderChat([line({})])
    await userEvent.hover(await screen.findByRole('img', { name: 'Waiting for translation' }))
    expect(await screen.findByText('Waiting for translation', { selector: '[role=tooltip]' })).toBeInTheDocument()
    release()
    expect(await screen.findByText('From Spanish · See original', { selector: '[role=tooltip]' })).toBeInTheDocument()
  })

  it('a message already in the language has no icon', async () => {
    const { requests } = mockServer((q) => ({ translatedText: q, detectedLanguage: q.map(() => ({ language: 'en' })) }))
    setAutoTranslate('Nearby', true)
    renderChat([line({ message: 'hello there' })])
    await waitFor(() => expect(requests).toHaveLength(1))
    await act(async () => {})
    expect(screen.queryByRole('img', { name: 'Waiting for translation' })).toBeNull()
    expect(screen.queryByRole('button', { name: /See original|Translate/ })).toBeNull()
  })

  it('never translates your own messages or system lines', async () => {
    const { requests } = mockServer(es({ hola: 'hello' }))
    setAutoTranslate('Nearby', true)
    renderChat([line({ sender: '0xME', id: 1 }), line({ sender: '', messageId: '', id: 2 })])
    await act(async () => {})
    expect(requests).toHaveLength(0)
    expect(screen.queryByRole('button', { name: 'Translate' })).toBeNull()
  })

  it('auto-translation is off by default; a line is translated by hand from its button', async () => {
    const { requests } = mockServer(es({ hola: 'hello' }))
    renderChat([line({})])
    await act(async () => {})
    expect(requests).toHaveLength(0)
    await userEvent.click(screen.getByRole('button', { name: 'Translate' }))
    expect(await screen.findByText('hello')).toBeInTheDocument()
  })

  it('the default applies to conversations not switched by hand', async () => {
    const { requests } = mockServer(es({ hola: 'hello' }))
    setAutoTranslateDefault(true)
    setAutoTranslate('0xbob', false)
    renderChat([line({ channel: '0xbob' })], '0xbob')
    await act(async () => {})
    expect(requests).toHaveLength(0)
    expect(screen.getByRole('button', { name: 'Auto-translate' })).toHaveAttribute('aria-pressed', 'false')
    await userEvent.click(screen.getByRole('button', { name: 'Auto-translate' }))
    expect(await screen.findByText('hello')).toBeInTheDocument()
    expect(JSON.parse(getPref(PREF.chatAutoTranslateChannels) ?? '{}')).toEqual({ '0xbob': true })
  })

  it('deleting a conversation\'s history puts it back on the default', async () => {
    setAutoTranslateDefault(true)
    setAutoTranslate('0xbob', false)
    setAutoTranslate('0xcat', false)
    renderChat([], '0xbob')
    await userEvent.click(screen.getByRole('button', { name: 'Conversation options' }))
    await userEvent.click(screen.getByText('Delete chat history'))
    expect(screen.getByRole('button', { name: 'Auto-translate' })).toHaveAttribute('aria-pressed', 'true')
    expect(JSON.parse(getPref(PREF.chatAutoTranslateChannels) ?? '{}')).toEqual({ '0xcat': false })
  })

  it('an auto-translation that fails leaves the message as it was; a hand one says so', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { requests } = mockServer(() => 503)
    setAutoTranslate('Nearby', true)
    renderChat([line({})])
    await waitFor(() => expect(requests).toHaveLength(1))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Translate' })).toBeInTheDocument())
    expect(screen.queryByRole('button', { name: /Couldn't translate/ })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Translate' }))
    expect(await screen.findByRole('button', { name: "Couldn't translate · Retry" })).toBeInTheDocument()
    expect(screen.getByText('hola')).toBeInTheDocument()
  })
})

describe('translation settings', () => {
  it('the Chat tab holds the language and the auto-translate default, kept as HUD prefs', async () => {
    render(
      <SettingsPanel
        settings={{ ...fakeSession().settings, open: true, list: [] }}
        bindings={fakeSession().bindings}
        profile={fakeProfileState()}
        onNavigate={vi.fn()}
      />
    )
    expect(screen.getByRole('button', { name: 'Translation Language' })).toHaveTextContent(/System \(/)
    await userEvent.click(screen.getByRole('button', { name: 'Translation Language' }))
    await userEvent.click(screen.getByRole('option', { name: 'Japanese' }))
    expect(getPref(PREF.chatTranslateLanguage)).toBe('ja')
    await userEvent.click(screen.getByRole('switch', { name: 'Auto-Translate New Conversations' }))
    expect(getPref(PREF.chatAutoTranslateDefault)).toBe('true')
  })

  it('resetting the Chat tab resets translation too, conversations switched by hand included', async () => {
    setTranslationLanguage('ja')
    setAutoTranslateDefault(true)
    setAutoTranslate('0xbob', false)
    render(
      <SettingsPanel
        settings={{ ...fakeSession().settings, open: true, list: [] }}
        bindings={fakeSession().bindings}
        profile={fakeProfileState()}
        onNavigate={vi.fn()}
      />
    )
    await userEvent.click(screen.getByRole('button', { name: /Reset all defaults/ }))
    expect(screen.getByRole('button', { name: 'Translation Language' })).toHaveTextContent(/System \(/)
    expect(screen.getByRole('switch', { name: 'Auto-Translate New Conversations' })).not.toBeChecked()
    expect(JSON.parse(getPref(PREF.chatAutoTranslateChannels) ?? '{}')).toEqual({})
  })
})
