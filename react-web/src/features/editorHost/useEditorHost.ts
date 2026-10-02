// Loads the scene editor into this page when the entry url asked for it (config.ts). Does nothing
// otherwise: no element, no script, no request.

import { useEffect, useRef } from 'react'
import { PAGE_DIR } from '../../lib/publicUrl'
import type { EngineSession } from '../session/useEngineSession'
import type { EditorSource } from './config'
import { guardUrlSync, mountEditorHost } from './host'

type ConsoleWindow = Window & { engine_console_command?: (line: string) => Promise<string> }

export function useEditorHost(source: EditorSource | null, session: EngineSession): void {
  const latest = useRef(session)
  latest.current = session

  const engineUp = session.login.engineReady
  useEffect(() => {
    if (source != null && engineUp) guardUrlSync(PAGE_DIR)
  }, [source, engineUp])

  const inWorld = session.phase === 'world'
  useEffect(() => {
    if (source == null || !inWorld) return
    mountEditorHost(source, PAGE_DIR, {
      engineConsole: (line) =>
        (window as ConsoleWindow).engine_console_command?.(line) ?? Promise.reject(new Error('the engine console is not available')),
      identity: () => {
        const { profile, login } = latest.current
        return { address: profile.data?.address ?? login.account, isGuest: profile.data?.isGuest ?? login.account == null }
      },
      setMode: (mode) => latest.current.editor.setMode(mode),
      travel: (realm, parcel) => latest.current.editor.travel(realm, parcel),
      scene: (action, url, hash) => latest.current.editor.scene(action, url, hash)
    })
  }, [source, inWorld])
}
