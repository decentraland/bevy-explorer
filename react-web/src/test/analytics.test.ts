// DOMAIN: the page→engine analytics handoff — events are stamped when they happen, wait for the
// engine's console, leave as `/track <base64 batch>` and are dropped only once the console answered.

import { beforeEach, describe, expect, it } from 'vitest'
import { attachEngine, BATCH_SIZE, encodeBatch, pending, screenViewed, track, type QueuedEvent } from '../lib/analytics'
import { PREF, setPref } from '../lib/prefs'

function decodeLine(line: string): QueuedEvent[] {
  const [command, payload] = line.split(' ')
  expect(command).toBe('/track')
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(payload), (c) => c.charCodeAt(0)))) as QueuedEvent[]
}

describe('page analytics', () => {
  beforeEach(() => setPref(PREF.analyticsQueue, ''))

  it('stamps each event with its time, and keeps it until the console answers', async () => {
    const from = pending().length
    screenViewed('ACCOUNT_HOME')
    track('App Opened', { start_kind: 'cold' })
    const queued = pending().slice(from)
    expect(queued.map((e) => e.event)).toEqual(['Screen Viewed', 'App Opened'])
    expect(queued[0].properties).toEqual({ screen_name: 'ACCOUNT_HOME' })
    expect(() => new Date(queued[0].timestamp).toISOString()).not.toThrow()

    const lines: string[] = []
    let answer: (reply: string) => void = () => {}
    attachEngine((line) => {
      lines.push(line)
      return new Promise((resolve) => {
        answer = resolve
      })
    })
    await Promise.resolve()
    expect(lines).toHaveLength(1)
    expect(decodeLine(lines[0]).map((e) => e.event)).toEqual(queued.map((e) => e.event))
    expect(pending().length).toBe(queued.length + from)
    answer('queued 2')
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(pending().slice(from)).toEqual([])
  })

  it('sends at most BATCH_SIZE per command, and a failed send is retried with what came after', async () => {
    const from = pending().length
    const batches: string[][] = []
    let fail: ((e: Error) => void) | null = null
    attachEngine((line) => {
      batches.push(decodeLine(line).map((e) => String(e.properties.button_text)))
      if (fail == null) {
        return new Promise((_, reject) => {
          fail = reject
        })
      }
      return Promise.resolve('queued')
    })
    for (let i = 0; i < BATCH_SIZE + 1; i++) track('Click Button', { button_text: String(i) })
    fail!(new Error('no console'))
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(batches.map((b) => b.length)).toEqual([1, BATCH_SIZE, 1])
    expect(batches[1][0]).toBe('0')
    expect(batches[2]).toEqual([String(BATCH_SIZE)])
    expect(pending().slice(from)).toEqual([])
  })

  it('encodes a batch as one base64 token of its JSON', () => {
    const batch: QueuedEvent[] = [{ event: 'Screen Viewed', properties: { screen_name: 'LOBBY', note: 'ñ' }, timestamp: 't' }]
    const token = encodeBatch(batch)
    expect(token).not.toMatch(/\s/)
    expect(decodeLine(`/track ${token}`)).toEqual(batch)
  })
})
