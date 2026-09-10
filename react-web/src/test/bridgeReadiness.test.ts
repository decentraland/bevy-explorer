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

/** BroadcastChannel delivery is asynchronous, and on a loaded CI runner it takes more than the one
 *  macrotask a bare setTimeout(0) waits for — so poll for the outcome rather than assume a turn.
 *  (A single `settle()` passed locally and failed in CI: bridgeReadiness at Build and Deploy Web.) */
const until = (check: () => boolean): Promise<void> =>
  vi.waitFor(() => {
    if (!check()) throw new Error('not yet')
  })


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
    await until(() => scene.kinds().includes('hello'))
  })

  it('holds page messages until the scene answers, then flushes them IN ORDER', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    const ch = track(new BridgeChannel(name, () => {}))

    ch.send({ kind: 'getProfile' })
    ch.send({ kind: 'getNotifications' })
    await until(() => scene.kinds().includes('hello'))
    // Nothing but the handshake has gone out.
    expect(scene.kinds().filter((k) => k !== 'hello')).toEqual([])
    expect(ch.isReady()).toBe(false)

    scene.announce()
    await until(() => ch.isReady())
    await until(() => scene.kinds().filter((k) => k !== 'hello').length === 2)
    expect(scene.kinds().filter((k) => k !== 'hello')).toEqual(['getProfile', 'getNotifications'])
  })

  it('sends straight through once ready', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    scene.autoAnswer = true
    const ch = track(new BridgeChannel(name, () => {}))
    await until(() => ch.isReady())

    ch.send({ kind: 'getProfile' })
    await until(() => scene.kinds().includes('getProfile'))
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

  it('reports once: re-arming after the report does not raise it again', async () => {
    vi.useFakeTimers()
    const name = channelName()
    const onUnavailable = vi.fn()
    const ch = track(new BridgeChannel(name, () => {}, onUnavailable))
    ch.expectReady()
    await vi.advanceTimersByTimeAsync(31_000)
    expect(onUnavailable).toHaveBeenCalledTimes(1)

    // The session arms on every phase change past launch (entering → world).
    ch.expectReady()
    await vi.advanceTimersByTimeAsync(60_000)
    expect(onUnavailable).toHaveBeenCalledTimes(1)
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
    await until(() => scene.kinds().includes('rpc:req'))

    // Sign-in reaches the engine while the scene is still booting; the scene's own request waits.
    expect(scene.kinds()).not.toContain('getProfile')
  })

  it('delivers scene messages to the listener, and ignores its own posts', async () => {
    const name = channelName()
    const scene = track(new FakeScene(name))
    const seen: SceneToPage[] = []
    const ch = track(new BridgeChannel(name, (m) => seen.push(m)))
    await until(() => scene.kinds().includes('hello'))

    ch.send({ kind: 'getProfile' }) // page→scene: must not come back to us
    scene.send({ kind: 'event', name: 'playerReady' })
    await until(() => seen.some((m) => m.kind === 'event'))
    expect(seen.some((m) => m.kind === 'getProfile' as unknown)).toBe(false)
  })
})

describe('an unreachable bridge is surfaced, not silent', () => {
  it('turns bridgeUnavailable into a dismissable dialog, not a crash', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().fatalError).toBeNull()

    h.driver.emit({ kind: 'bridgeUnavailable' })
    expect(h.session().fatalError?.source).toBe('bridge')
    expect(h.session().fatalError?.message).toMatch(/bridge/i)
  })

  it('does not overwrite an error already on screen', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'bridgeUnavailable' })
    const first = h.session().fatalError
    h.driver.emit({ kind: 'bridgeUnavailable' })
    expect(h.session().fatalError).toBe(first)
  })
})
