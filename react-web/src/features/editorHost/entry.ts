// The HUD's way into the scene editor: what the Create buttons (menu top bar, sidebar rail) read.
// null, the default, is a page with no editor: the buttons are not rendered.

import { createContext } from 'react'

export interface EditorEntry {
  open: () => void
  /** The editor package is downloading. */
  loading: boolean
}

export const EditorEntryContext = createContext<EditorEntry | null>(null)
