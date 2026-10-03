// Hosts the scene editor in this page (config.ts): its home renders in the Create page, opened
// from a Create button or once in-world when the entry url asked for it. Until then it does
// nothing: no element, no script, no request.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { bridgeChannelName } from '../../engine/protocol'
import { DEFAULT_REALM } from '../../lib/baseDomain'
import { PAGE_DIR } from '../../lib/publicUrl'
import { getStoredLogin, rootAddress } from '../auth/sso'
import type { EngineSession } from '../session/useEngineSession'
import { editorEntry, type EditorSource } from './config'
import { confirmDeployment } from './DeployConfirm'
import type { EditorEntry } from './entry'
import type { EditorHostScript } from './host/editorHost'
import type { EditorPackage } from './host/host'
// Vite's worker pipeline is what bundles a file on its own; this one is loaded by script tag.
import hostScriptUrl from './host/editorHost.ts?worker&url'

type HostWindow = Window & {
  engine_console_command?: (line: string) => Promise<string>
  __dclEditorHostScript?: (script: EditorHostScript) => void
  __bridgeSession?: string
}

let hostScript: Promise<EditorHostScript> | null = null
let attempts = 0
/** The host script, added to the page the first time it is needed. */
function loadHostScript(): Promise<EditorHostScript> {
  hostScript ??= new Promise<EditorHostScript>((resolve, reject) => {
    const w = window as HostWindow
    const script = document.createElement('script')
    script.type = 'module'
    // a module that failed to load stays failed under its url
    script.src = attempts++ === 0 ? hostScriptUrl : `${hostScriptUrl}${hostScriptUrl.includes('?') ? '&' : '?'}retry=${attempts}`
    w.__dclEditorHostScript = (loaded) => {
      delete w.__dclEditorHostScript
      resolve(loaded)
    }
    script.onerror = () => {
      delete w.__dclEditorHostScript
      script.remove()
      hostScript = null
      reject(new Error('the editor host failed to load'))
    }
    document.head.appendChild(script)
  })
  return hostScript
}

export function useEditorHost(source: EditorSource | null, session: EngineSession): EditorEntry | null {
  const latest = useRef(session)
  latest.current = session
  // from the ENTRY url: the engine's url sync rewrites location.search
  const [entry] = useState(() => editorEntry(location.search))
  const flag = entry.project ?? entry.open
  const [loading, setLoading] = useState(false)

  // with `?editor`, before the engine's first url sync would drop the flag
  const engineUp = session.login.engineReady
  useEffect(() => {
    if (source == null || !entry.open || !engineUp) return
    loadHostScript().then(
      (host) => host.guardUrlSync(PAGE_DIR, flag),
      () => {}
    )
  }, [source, entry, flag, engineUp])

  const load = useCallback(
    async (): Promise<EditorPackage> => {
      if (source == null) throw new Error('no editor on this page')
      setLoading(true)
      bridgeChannelName() // seeds __bridgeSession when nothing has yet
      try {
        const host = await loadHostScript()
        return await host.loadEditor(
          source,
          PAGE_DIR,
          {
            busSession: (window as HostWindow).__bridgeSession ?? '',
            defaultRealm: DEFAULT_REALM,
            engineConsole: (line) =>
              (window as HostWindow).engine_console_command?.(line) ?? Promise.reject(new Error('the engine console is not available')),
            identity: () => {
              const { profile, login } = latest.current
              return { address: profile.data?.address ?? login.account, isGuest: profile.data?.isGuest ?? login.account == null }
            },
            login: () => {
              const me = latest.current.profile.data
              const stored = getStoredLogin()
              if (me == null || me.isGuest || stored == null) return null
              return rootAddress(stored.identity).toLowerCase() === me.address.toLowerCase() ? stored.identity : null
            },
            confirmDeployment,
            setMode: (mode) => latest.current.editor.setMode(mode),
            showCreatePage: (open) => latest.current.create.show(open),
            travel: (realm, parcel) => latest.current.editor.travel(realm, parcel),
            scene: (action, url, hash) => latest.current.editor.scene(action, url, hash)
          },
          flag
        )
      } finally {
        setLoading(false)
      }
    },
    [source, flag]
  )

  // once: Create opens when the player is first in-world
  const auto = useRef(entry.open)
  const inWorld = session.phase === 'world'
  useEffect(() => {
    if (source == null || !auto.current || !inWorld) return
    auto.current = false
    latest.current.create.show(true)
  }, [source, inWorld])

  return useMemo(() => (source == null ? null : { load, loading }), [source, load, loading])
}
