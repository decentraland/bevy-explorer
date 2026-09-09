import { describe, it, expect, vi, afterEach } from 'vitest'
import { BridgeChannel } from '../engine/bridgeChannel'
import type { Envelope, PageToScene, SceneToPage } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// TRANSPORT: the page↔scene handshake. A BroadcastChannel drops anything posted before the other
// end subscribes, so the page holds its messages until the scene answers `hello` (issue #1233).

/** A stand-in for the bridge scene: records what the page posts, answers when told to. */
class FakeScene {
  readonly received: PageToScene[] = []
  private readonly ch: BroadcastChannel
  /** Answer every hello, like the real scene does. */
  autoAnswer = false

  constructor(name: string) {
    this.ch = new BroadcastChannel(name)
    this.ch.onmessage = (e: MessageEvent<Envelope>) => {
      const env = e.data
      if (env?.to !== 'scene') return
      this.received.push(env.msg)
      if (this.autoAnswer && env.msg.kind === 'hello') this.announce()
    }
  }

  announce(): void {
    this.send({ kind: 'bridgeReady' })
  }

  send(msg: SceneToPage): void {
    this.ch.postMessage({ to: 'page', msg } satisfies Envelope)
  }

  kinds(): string[] {
    return this.received.map((m) => m.kind)
  }

  close(): void {
    this.ch.close()
  }
}

/** BroadcastChannel delivery is a task, not microtask — let it land. */
const settle = (): Promise<void> => new Promise((r) => setTimeout(r, 0))

let open: Array<{ close: () => void }> = []
const track = <T extends { close: () => void }>(x: T): T => {
  open.push(x)
  return x
}
afterEach(() => {
  open.forEach((c) => c.close())
  open = []
  vi.useRealTimers()
})

let seq = 0
const channelName = (): string => `test-bridge-${seq++}`

describe('bridge readiness handshake', () => {
  it('says hello on construction', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    track(new BridgeChannel(name, () => {}))
    await settle()
    expect(scene.kinds()).toContain('hello')
  })

  it('holds page messages until the scene answers, then flushes them IN ORDER', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    const ch = track(new BridgeChannel(name, () => {}))

    ch.send({ kind: 'getProfile' })
    ch.send({ kind: 'getNotifications' })
    await settle()
    // Nothing but the handshake has gone out.
    expect(scene.kinds().filter((k) => k !== 'hello')).toEqual([])
    expect(ch.isReady()).toBe(false)

    scene.announce()
    await settle()
    expect(ch.isReady()).toBe(true)
    expect(scene.kinds().filter((k) => k !== 'hello')).toEqual(['getProfile', 'getNotifications'])
  })

  it('sends straight through once ready', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    scene.autoAnswer = true
    const ch = track(new BridgeChannel(name, () => {}))
    await settle()

    ch.send({ kind: 'getProfile' })
    await settle()
    expect(scene.kinds()).toContain('getProfile')
  })

  it('keeps saying hello until answered, so a scene that starts LATE still hears it', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const ch = track(new BridgeChannel(name, () => {}))
    ch.send({ kind: 'getProfile' })

    // The scene subscribes only now, having missed every hello so far.
    const scene = track(new FakeScene(name))
    scene.autoAnswer = true
    await vi.advanceTimersByTimeAsync(600)

    expect(scene.kinds()).toContain('hello')
    expect(scene.kinds()).toContain('getProfile')
    expect(ch.isReady()).toBe(true)
  })

  it('says nothing while the bridge is not yet expected — an idle login screen has no realm', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const onUnavailable = vi.fn()
    track(new BridgeChannel(name, () => {}, onUnavailable))

    // Before sign-in there is legitimately no bridge scene on web.
    await vi.advanceTimersByTimeAsync(120_000)
    expect(onUnavailable).not.toHaveBeenCalled()
  })

  it('reports the bridge unavailable once expected, instead of queueing forever', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const onUnavailable = vi.fn()
    const ch = track(new BridgeChannel(name, () => {}, onUnavailable))
    ch.expectReady()

    await vi.advanceTimersByTimeAsync(29_000)
    expect(onUnavailable).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(2_000)
    expect(onUnavailable).toHaveBeenCalledTimes(1)
    expect(ch.isReady()).toBe(false)
  })

  it('does not report unavailable once the scene has answered', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const scene = track(new FakeScene(name))
    scene.autoAnswer = true
    const onUnavailable = vi.fn()
    const ch = track(new BridgeChannel(name, () => {}, onUnavailable))
    ch.expectReady()

    await vi.advanceTimersByTimeAsync(60_000)
    expect(onUnavailable).not.toHaveBeenCalled()
  })

  it('arming after the scene already answered never fires', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const scene = track(new FakeScene(name))
    scene.autoAnswer = true
    const onUnavailable = vi.fn()
    const ch = track(new BridgeChannel(name, () => {}, onUnavailable))
    await vi.advanceTimersByTimeAsync(300)

    ch.expectReady()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(onUnavailable).not.toHaveBeenCalled()
  })

  it('sendNow bypasses the queue — the login RPCs the engine shim answers must not wait', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    const ch = track(new BridgeChannel(name, () => {}))

    ch.send({ kind: 'getProfile' }) // belongs to the scene: queued
    ch.sendNow({ kind: 'rpc:req', id: 'a', method: 'loginPrevious' })
    await settle()

    // Sign-in reaches the engine while the scene is still booting; the scene's own request waits.
    expect(scene.kinds()).toContain('rpc:req')
    expect(scene.kinds()).not.toContain('getProfile')
  })

  it('delivers scene messages to the listener, and ignores its own posts', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    const seen: SceneToPage[] = []
    const ch = track(new BridgeChannel(name, (m) => seen.push(m)))
    await settle()

    ch.send({ kind: 'getProfile' }) // page→scene: must not come back to us
    scene.send({ kind: 'event', name: 'playerReady' })
    await settle()

    expect(seen.map((m) => m.kind)).toContain('event')
    expect(seen.some((m) => m.kind === 'getProfile' as unknown)).toBe(false)
  })
})

describe('an unreachable bridge is surfaced, not silent', () => {
  it('turns bridgeUnavailable into a dismissable crash the user can act on', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().fatalError).toBeNull()

    h.driver.emit({ kind: 'bridgeUnavailable' })
    expect(h.session().fatalError?.source).toBe('runtime')
    expect(h.session().fatalError?.message).toMatch(/bridge/i)
  })

  it('does not overwrite a crash already on screen', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'bridgeUnavailable' })
    const first = h.session().fatalError
    h.driver.emit({ kind: 'bridgeUnavailable' })
    expect(h.session().fatalError).toBe(first)
  })
})
