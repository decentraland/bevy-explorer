// The editor host's own requests to the bridge scene; ids start above the HUD session's travel ids.

import { bridgeChannelName, type Envelope, type PageToScene, type SceneToPage } from '../../../engine/protocol'

let seq = 1_000_000
const waiting = new Map<number, { resolve: () => void; reject: (e: Error) => void }>()
let channel: BroadcastChannel | null = null

function settle(msg: SceneToPage): void {
  let id: number
  let error: string | undefined
  switch (msg.kind) {
    case 'travelResult':
      id = msg.travelId
      error = msg.message
      break
    case 'editorSceneResult':
      id = msg.id
      error = msg.error
      break
    default:
      return
  }
  const waiter = waiting.get(id)
  if (waiter == null) return
  waiting.delete(id)
  if (msg.ok) waiter.resolve()
  else waiter.reject(new Error(error ?? 'unknown error'))
}

// a lost reply must not wedge the editor until reload; spawn itself waits up to 30 s for the scene
const TIMEOUT_MS = 45_000

function request(make: (id: number) => PageToScene): Promise<void> {
  if (channel == null) {
    channel = new BroadcastChannel(bridgeChannelName())
    channel.onmessage = (e: MessageEvent<Envelope>) => {
      if (e.data?.to === 'page') settle(e.data.msg)
    }
  }
  const id = ++seq
  const sent = channel
  return new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      waiting.delete(id)
      reject(new Error('the bridge scene did not answer'))
    }, TIMEOUT_MS)
    waiting.set(id, {
      resolve: () => {
        clearTimeout(timer)
        resolve()
      },
      reject: (e) => {
        clearTimeout(timer)
        reject(e)
      }
    })
    sent.postMessage({ to: 'scene', msg: make(id) } satisfies Envelope)
  })
}

/** A realm change that reports how it ended; with a parcel, lands on it. */
export const bridgeTravel = (realm: string, parcel?: { x: number; y: number }): Promise<void> =>
  request((travelId) => (parcel ? { kind: 'teleport', realm, ...parcel, travelId } : { kind: 'changeRealm', realm, travelId }))

/** Spawn or kill the editor's own scene (EditorSceneRequest). */
export const bridgeScene = (action: 'spawn' | 'kill', source: string, hash: string): Promise<void> =>
  request((id) => ({ kind: 'editorScene', id, action, source, hash }))
