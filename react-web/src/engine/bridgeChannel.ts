// The page's half of the bridge handshake, shared by both drivers (BridgeClient on native/CEF,
// EngineDriver on web) because the failure it prevents is a property of the transport, not of
// either driver.
//
// A BroadcastChannel has no buffering and no connection: a message posted before the other end
// subscribes is simply gone, and neither side is told. The page and the bridge scene start
// independently — the engine boots the scene, the browser (or CEF) loads the page — so either can
// win the race. Anything sent into that gap is lost silently, which is how the HUD ends up with
// panels that never fill and no error to explain why (issue #1233).
//
// The handshake: the page says `hello` (repeatedly, since that too can be lost) until the scene
// answers `bridgeReady`. Until then page→scene messages are held and then flushed in order, so
// nothing is posted into the void. The scene answers every `hello`, so a page that reloads — or
// arrives long after the scene — still gets an answer, and domains holding one-shot state can
// re-send it to the newcomer.

import type { Envelope, PageToScene, SceneToPage } from './protocol'

const HELLO_INTERVAL_MS = 250

/** Only armed by `expectReady()`: before the engine is launched there is legitimately no bridge on
 *  web, and a clock started at construction just accuses an idle login screen or picker. */
const READY_TIMEOUT_MS = 30_000

export class BridgeChannel {
  private readonly ch: BroadcastChannel
  private ready = false
  /** Page→scene messages posted before the scene answered, flushed in order once it does. */
  private readonly queue: PageToScene[] = []
  private helloTimer: ReturnType<typeof setInterval> | null = null
  private timeoutTimer: ReturnType<typeof setTimeout> | null = null
  /** `expectReady()` is one-shot: a fault already reported must not come back on the next arm. */
  private expected = false

  constructor(
    channelName: string,
    private readonly onMessage: (msg: SceneToPage) => void,
    private readonly onUnavailable?: () => void
  ) {
    this.ch = new BroadcastChannel(channelName)
    this.ch.onmessage = (e: MessageEvent<Envelope>) => {
      const env = e.data
      if (env?.to !== 'page') return // ignore our own / scene-addressed posts
      if (env.msg.kind === 'bridgeReady') this.markReady()
      this.onMessage(env.msg)
    }
    this.hello()
    this.helloTimer = setInterval(() => this.hello(), HELLO_INTERVAL_MS)
  }

  /** Start the clock: the engine has been launched at a realm, so a still-absent bridge is a fault
   *  worth showing rather than a normal wait. One-shot — the session calls it on every phase change
   *  past launch, and a fault already reported must not be raised again by the next one. Keeps
   *  saying hello afterwards, so a scene that turns up late still recovers the queue. */
  expectReady(): void {
    if (this.ready || this.expected) return
    this.expected = true
    this.timeoutTimer = setTimeout(() => {
      this.timeoutTimer = null
      if (!this.ready) this.onUnavailable?.()
    }, READY_TIMEOUT_MS)
  }

  send(msg: PageToScene): void {
    if (!this.ready) {
      this.queue.push(msg)
      return
    }
    this.post(msg)
  }

  /** Post immediately, skipping the handshake. Only for messages something OTHER than the bridge
   *  scene answers: on native the engine's boot shim serves the login RPCs, so queueing them would
   *  make sign-in wait for a scene it does not need. */
  sendNow(msg: PageToScene): void {
    this.post(msg)
  }

  isReady(): boolean {
    return this.ready
  }

  /** Named to match BroadcastChannel, since it stands in for one at both call sites. */
  close(): void {
    this.stopHello()
    if (this.timeoutTimer != null) clearTimeout(this.timeoutTimer)
    this.timeoutTimer = null
    this.ch.close()
  }

  private markReady(): void {
    // The scene answers every hello, so this fires more than once; only the first does anything.
    if (this.ready) return
    this.ready = true
    this.stopHello()
    if (this.timeoutTimer != null) clearTimeout(this.timeoutTimer)
    this.timeoutTimer = null
    for (const msg of this.queue.splice(0)) this.post(msg)
  }

  /** The handshake itself bypasses the queue — it is what opens the queue. */
  private hello(): void {
    this.post({ kind: 'hello' })
  }

  private post(msg: PageToScene): void {
    this.ch.postMessage({ to: 'scene', msg } satisfies Envelope)
  }

  private stopHello(): void {
    if (this.helloTimer != null) clearInterval(this.helloTimer)
    this.helloTimer = null
  }
}
