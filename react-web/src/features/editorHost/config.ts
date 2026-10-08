// Whether this page offers the scene editor. The rest (the package, its services, the project the
// entry url names) is read in the Create page's own chunk (source.ts), so it costs the HUD nothing.

import { createContext } from 'react'
import type { EditorPin } from './source'

/** The released package (README "Releasing the editor") and the hosts besides loopback that offer
 *  it; null = none, so a production build carries no editor at all. */
export const PINNED_EDITOR = {
  base: 'https://cdn.jsdelivr.net/npm/@dcl-regenesislabs/web-editor@0.1.1-manual.202610072104.commit-80c8e58/',
  editorJsIntegrity: 'sha384-GtAjJnQjyvlZI9wKEg8FKxrZhg2DhJbbdC3FpY9pNqjgypYgrT20j261lNPG65XT',
  editorSceneEntity: 'bafkreidoizpb4vtisnjylzsbcst45lgxx65n4sh3ekv4rtx4ybdhq6izwm',
  hosts: ['decentraland.zone']
} as (EditorPin & { hosts: string[] }) | null

// false folds every Create entry point out of a production build that has no pin
export const EDITOR_BUILD = import.meta.env.DEV || PINNED_EDITOR != null

export const isLoopback = (hostname: string): boolean => hostname === 'localhost' || hostname === '127.0.0.1'

export const editorOffered = (hostname: string): boolean =>
  EDITOR_BUILD && (isLoopback(hostname) || (PINNED_EDITOR?.hosts.includes(hostname) ?? false))

/** Whether the Create entry points (menu top bar, sidebar rail) are offered. */
export const EditorOffered = createContext(false)
