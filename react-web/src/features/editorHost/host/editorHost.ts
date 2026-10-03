// The editor host as a script of its own: useEditorHost adds it when Create is first opened
// and takes it through a one-shot hook, so neither the host nor the signing code is in the HUD
// bundle.

import { guardUrlSync, loadEditor } from './host'

export interface EditorHostScript {
  guardUrlSync: typeof guardUrlSync
  loadEditor: typeof loadEditor
}

type HookWindow = Window & { __dclEditorHostScript?: (script: EditorHostScript) => void }

;(window as HookWindow).__dclEditorHostScript?.({ guardUrlSync, loadEditor })
