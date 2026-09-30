// One progress value for the whole loader. Entering the world waits on several things in turn: the
// realm, the scene's glTF models (the engine reports how many are still pending, and scenes spawn
// them in batches, so the count can jump back up), the player spawn and the render settle. Each
// stage has a floor, and the shown value only ever rises until the loader goes away.

import type { SceneLoadingState } from '../../engine/protocol'

export interface LoadingInput {
  scene: SceneLoadingState | null
  playerReady: boolean
  revealing: boolean
  travelling: boolean
}

const CONNECTING = 5
const MODELS_FROM = 10
const MODELS_TO = 80
const MODELS_SETTLED = 82
const SCENE_SHOWN = 85
const PLAYER_READY = 92
const RENDER_SETTLE = 96

export function createLoadingProgress(): { next: (i: LoadingInput) => number; reset: () => void } {
  let shown = 0
  let peak = 0
  let counted = false

  const stage = ({ scene, playerReady, revealing, travelling }: LoadingInput): number => {
    if (travelling || scene == null || !scene.realmConnected) return CONNECTING
    if (scene.visible) {
      const pending = scene.pendingAssets
      // The engine drops the count once the scene's models are settled.
      if (pending == null) return counted ? MODELS_SETTLED : MODELS_FROM
      counted = true
      peak = Math.max(peak, pending)
      return peak === 0 ? MODELS_TO : MODELS_FROM + (MODELS_TO - MODELS_FROM) * (1 - pending / peak)
    }
    if (revealing) return RENDER_SETTLE
    return playerReady ? PLAYER_READY : SCENE_SHOWN
  }

  return {
    next: (i) => {
      shown = Math.max(shown, Math.round(stage(i)))
      return shown
    },
    reset: () => {
      shown = 0
      peak = 0
      counted = false
    }
  }
}
