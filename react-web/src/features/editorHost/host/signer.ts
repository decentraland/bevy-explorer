// The signing code as a script of its own: host.ts adds it the first time the editor signs, so
// neither the HUD nor the Create page's chunk carries it or its curve library.

import { signDeployment, signFetch } from './sign'

export interface Signer {
  signFetch: typeof signFetch
  signDeployment: typeof signDeployment
}

type HookWindow = Window & { __dclEditorSigner?: (signer: Signer) => void }

;(window as HookWindow).__dclEditorSigner?.({ signFetch, signDeployment })
