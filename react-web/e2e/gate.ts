// What the editor's gates share: where the page and its servers are, and a guard against writes to real servers.

import type { BrowserContext } from '@playwright/test'
import { GATE_PORTS } from './ports'

export const HOME_REALM = '/gate-home'
export const NAV = 'nav[aria-label="Main navigation"]'
export const UI = '#dcl-editor-host > #editor-ui-host'
/** The Create page's body, where the editor renders its home. */
export const HOME = '#dcl-editor-home'
/** The editor's home (a shadow root inside the Create page's body). */
export const SCENES = `${HOME} #editor-ui-host`
/** The page's entry path: the dev server's root, or `GATE_ENTRY` (a production layout's no-slash entry). */
export const ENTRY = process.env.GATE_ENTRY ?? '/'
/** Where the page loads the editor package from: its own /editor/, or `GATE_EDITOR_BASE` (a CDN). */
export const editorBase = (origin: string): string => process.env.GATE_EDITOR_BASE ?? `${origin}/editor/`
/** playwright.gate.config.ts starts it. */
export const PROJECTS = `http://localhost:${GATE_PORTS.service}`
/** Points the page at the gate's own servers: its bridge scene (a production build bundles its
 *  own) and its project service. */
export const SERVERS = [
  ...(process.env.GATE_ENTRY ? [] : [`bridgePort=${GATE_PORTS.bridge}`]),
  `editor-projects=${encodeURIComponent(PROJECTS)}`
].join('&')

export async function keepOffProduction(context: BrowserContext, blocked: string[]): Promise<void> {
  await context.route('https://api.segment.io/**', (route) => route.abort())
  await context.route(/^https?:\/\/(?!localhost[:/]|127\.0\.0\.1[:/])[^/]+\/(.*\/)?entities\/?(\?.*)?$/, (route) => {
    const request = route.request()
    if (['GET', 'HEAD', 'OPTIONS'].includes(request.method())) return route.continue()
    blocked.push(`${request.method()} ${request.url()}`)
    return route.abort()
  })
}
