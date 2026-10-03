// The HUD's way into the scene editor: what the Create entry points (menu top bar, sidebar rail)
// and the Create page read. null, the default, is a page with no editor: Create is not offered.

import { createContext } from 'react'
import type { EditorPackage } from './host/host'

export interface EditorEntry {
  /** The editor package, loaded (and the host published) the first time. */
  load: () => Promise<EditorPackage>
  /** The editor package is downloading. */
  loading: boolean
}

export const EditorEntryContext = createContext<EditorEntry | null>(null)
