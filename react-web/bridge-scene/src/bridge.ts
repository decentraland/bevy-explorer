// Bridge framework: the transport + a tiny registration API the domain modules use.
//
// Architecture (read top-down):
//   React DOM HUD  ──BroadcastChannel('bevy-ui-bridge')──►  this scene  ──BevyApi──►  engine
//
// Each `domains/*.ts` module registers two kinds of wiring through `Ctx`:
//   • ctx.on('<kind>', handler)  — a request coming FROM React (e.g. 'getProfile')
//   • ctx.push(system)           — a per-frame system that PUSHES updates TO React
// so for any piece of HUD data you can open one domain file and see exactly where it comes
// from. Messages are typed by the SHARED wire protocol (react-web/src/engine/protocol.ts) —
// the same file the React page uses — so changing a message on either side breaks the other.

import { engine } from '@dcl/sdk/ecs'
import type { Envelope, PageToScene, SceneToPage } from '../../src/engine/protocol'

const CHANNEL = 'bevy-ui-bridge'

// BroadcastChannel is a runtime global injected only into the super-user (--ui) scene
// sandbox; it is not in the SDK type defs.
declare const BroadcastChannel: new (name: string) => {
  postMessage: (msg: unknown) => void
  onmessage: ((e: { data: unknown }) => void) | null
}

/** A handler for a specific request kind `K`, receiving exactly that message variant. */
type HandlerFor<K extends PageToScene['kind']> = (
  msg: Extract<PageToScene, { kind: K }>
) => void | Promise<void>

export type Ctx = {
  /** Push a typed message to the React host page. */
  send: (msg: SceneToPage) => void
  /** Handle an incoming request of `kind` from React; `msg` is narrowed to that variant. */
  on: <K extends PageToScene['kind']>(kind: K, handler: HandlerFor<K>) => void
  /** Run a per-frame system (for polling / streams that push to React). */
  push: (system: (dt: number) => void) => void
}

type AnyHandler = (msg: PageToScene) => void | Promise<void>

export function startBridge(register: (ctx: Ctx) => void): void {
  let channel: { postMessage: (m: unknown) => void; onmessage: ((e: { data: unknown }) => void) | null }
  try {
    channel = new BroadcastChannel(CHANNEL)
  } catch (e) {
    console.log('[bridge] BroadcastChannel unavailable (not super-user?)', e)
    return
  }

  const handlers = new Map<string, AnyHandler>()
  const ctx: Ctx = {
    send: (msg) => {
      channel.postMessage({ to: 'page', msg } satisfies Envelope)
    },
    // The dispatcher routes by `kind`, so the narrowed handler always receives a message of
    // the kind it registered for — the single cast here is sound.
    on: (kind, handler) => {
      handlers.set(kind, handler as AnyHandler)
    },
    push: (system) => {
      engine.addSystem(system)
    }
  }

  register(ctx)

  channel.onmessage = (e): void => {
    const env = e.data as Envelope | null
    if (env === null || env.to !== 'scene') return
    const handler = handlers.get(env.msg.kind)
    if (handler == null) return
    try {
      const r = handler(env.msg)
      if (r instanceof Promise) {
        r.catch((err) => {
          console.error(`[bridge] ${env.msg.kind} failed`, err)
        })
      }
    } catch (err) {
      console.error(`[bridge] ${env.msg.kind} failed`, err)
    }
  }
}
