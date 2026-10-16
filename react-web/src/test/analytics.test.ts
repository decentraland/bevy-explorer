// DOMAIN: the page's analytics queue (lib/analytics.ts) — what a queued event carries, that it
// survives a page reload through the store, and how the engine's /track takes it.

import { describe, expect, it, vi } from 'vitest'
import { BATCH_SIZE, createAnalytics, type QueuedEvent, type QueueStore } from '../lib/analytics'

// What the engine's /track does with the token (crates/analytics/src/page_events.rs).
function decodeBatch(command: string): QueuedEvent[] {
  const bytes = Uint8Array.from(atob(command.replace(/^\/track\s+/, '')), (c) => c.charCodeAt(0))
  return JSON.parse(new TextDecoder().decode(bytes)) as QueuedEvent[]
}

function memoryStore(): QueueStore & { json: string | null } {
  return {
    json: null,
    load() {
      return this.json
    },
    save(json) {
      this.json = json
    }
  }
}

describe('analytics queue', () => {
  it('keeps each event as stamped at call time across a reload, then drains it as one /track batch', async () => {
    const store = memoryStore()
    const first = createAnalytics(store, 'session-a')
    first.screenViewed('ACCOUNT_HOME')
    first.clickButton('PLAY_GUEST', 'ACCOUNT_HOME', { from: 'test' })
    const stamped = first.pending().map((e) => ({ ...e }))
    expect(stamped).toHaveLength(2)
    expect(stamped[0]).toMatchObject({ event: 'Screen Viewed', properties: { screen_name: 'ACCOUNT_HOME' }, session_id: 'session-a' })
    expect(stamped[1].properties).toEqual({ button_text: 'PLAY_GUEST', screen_name: 'ACCOUNT_HOME', extra_properties: '{"from":"test"}' })
    expect(Date.parse(stamped[0].created_at)).not.toBeNaN()
    expect(stamped[0].message_id).not.toBe(stamped[1].message_id)
    await Promise.resolve() // the store is written on the microtask after the call

    const reloaded = createAnalytics(store, 'session-b')
    const commands: string[] = []
    reloaded.attachEngine(async (line) => {
      commands.push(line)
      return 'ok 2'
    })
    await reloaded.flush()
    expect(commands).toHaveLength(1)
    expect(commands[0].startsWith('/track ')).toBe(true)
    expect(commands[0].split(/\s/)).toHaveLength(2)
    expect(decodeBatch(commands[0])).toEqual(stamped)
    expect(reloaded.pending()).toEqual([])
    expect(JSON.parse(store.json ?? 'x')).toEqual([])
  })

  it('a failed /track keeps the events for the next flush, in batches the engine accepts', async () => {
    const analytics = createAnalytics(memoryStore(), 's')
    for (let i = 0; i < BATCH_SIZE + 5; i++) analytics.screenViewed(`S${i}`)
    const engine = vi.fn<(line: string) => Promise<string>>().mockRejectedValue(new Error('engine not initialized'))
    analytics.attachEngine(engine)
    await analytics.flush()
    expect(engine).toHaveBeenCalled()
    expect(analytics.pending()).toHaveLength(BATCH_SIZE + 5)

    engine.mockResolvedValue('ok')
    const failed = engine.mock.calls.length
    await analytics.flush()
    expect(engine).toHaveBeenCalledTimes(failed + 2)
    expect(decodeBatch(engine.mock.calls[failed][0])).toHaveLength(BATCH_SIZE)
    expect(decodeBatch(engine.mock.calls[failed + 1][0])).toHaveLength(5)
    expect(analytics.pending()).toEqual([])
  })
})
