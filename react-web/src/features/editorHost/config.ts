// Whether this page offers the scene editor. The rest (the package, its services, the project the
// entry url names) is read in the Create page's own chunk (source.ts), so it costs the HUD nothing.

import { createContext } from 'react'
import type { EditorRelease } from './source'

/** The released package the page follows (README "Releasing the editor") and the hosts besides
 *  loopback that offer it; null = none, so a production build carries no editor at all. */
export const RELEASED_EDITOR = {
  package: '@dcl-regenesislabs/web-editor',
  tag: 'latest',
  hosts: ['decentraland.zone']
} as (EditorRelease & { hosts: string[] }) | null

// false folds every Create entry point out of a production build that has no release
export const EDITOR_BUILD = import.meta.env.DEV || RELEASED_EDITOR != null

export const isLoopback = (hostname: string): boolean => hostname === 'localhost' || hostname === '127.0.0.1'

export const editorOffered = (hostname: string): boolean =>
  EDITOR_BUILD && (isLoopback(hostname) || (RELEASED_EDITOR?.hosts.includes(hostname) ?? false))

/** Whether the Create entry points (menu top bar, sidebar rail) are offered. */
export const EditorOffered = createContext(false)
