// The scene editor's host and Create page: a chunk of its own, loaded only once Create is wanted.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { bridgeChannelName } from '../../engine/protocol'
import { BASE_DOMAIN, DEFAULT_REALM } from '../../lib/baseDomain'
import { PAGE_DIR } from '../../lib/publicUrl'
import { getLogin, rootAddress } from '../auth/sso'
import type { EngineSession } from '../session/useEngineSession'
import { CreatePage } from './CreatePage'
import { confirmDeployment, confirmUndeploy } from './DeployConfirm'
import { bridgeScene, bridgeTravel } from './host/bridge'
import { guardUrlSync, loadEditor, type EditorPackage } from './host/host'
import { editorEntry, editorSource, hasEditor, type EditorSource } from './source'

type HostWindow = Window & {
  engine_console_command?: (line: string) => Promise<string>
  __bridgeSession?: string
}

export interface EditorEntry {
  /** The editor package, loaded (and the host published) the first time. */
  load: () => Promise<EditorPackage>
  /** The editor package is downloading. */
  loading: boolean
}

/** `entrySearch`: the ENTRY url's query, from before the engine's url sync rewrote it. */
export function useEditorHost(entrySearch: string, session: EngineSession): EditorEntry | null {
  const latest = useRef(session)
  latest.current = session
  const [{ offered, entry }] = useState(() => ({ offered: hasEditor(location.hostname), entry: editorEntry(entrySearch) }))
  const flag = entry.project ?? entry.open
  const [loading, setLoading] = useState(false)
  // resolved once per page; a failed resolution is retried by the next load
  const source = useRef<Promise<EditorSource | null> | null>(null)

  // with `?editor`, before the engine's first url sync would drop the flag
  const engineUp = session.login.engineReady
  useEffect(() => {
    if (offered && entry.open && engineUp) guardUrlSync(PAGE_DIR, flag)
  }, [offered, entry, flag, engineUp])

  const load = useCallback(
    async (): Promise<EditorPackage> => {
      if (!offered) throw new Error('no editor on this page')
      setLoading(true)
      bridgeChannelName() // seeds __bridgeSession when nothing has yet
      try {
        source.current ??= editorSource(entrySearch, location.hostname, PAGE_DIR, BASE_DOMAIN)
        const resolved = await source.current.catch((e: unknown) => {
          source.current = null
          throw e
        })
        if (resolved == null) throw new Error('no editor on this page')
        return await loadEditor(
          resolved,
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
            login: async () => {
              const me = latest.current.profile.data
              const stored = await getLogin()
              if (me == null || me.isGuest || stored == null) return null
              return rootAddress(stored.identity).toLowerCase() === me.address.toLowerCase() ? stored.identity : null
            },
            confirmDeployment,
            confirmUndeploy,
            setMode: (mode) => latest.current.editor.setMode(mode),
            showCreatePage: (open) => latest.current.create.show(open),
            travel: bridgeTravel,
            scene: bridgeScene
          },
          flag
        )
      } finally {
        setLoading(false)
      }
    },
    [offered, entrySearch, flag]
  )

  // once: Create opens when the player is first in-world
  const auto = useRef(entry.open)
  const inWorld = session.phase === 'world'
  useEffect(() => {
    if (!offered || !auto.current || !inWorld) return
    auto.current = false
    latest.current.create.show(true)
  }, [offered, inWorld])

  return useMemo(() => (offered ? { load, loading } : null), [offered, load, loading])
}

export default function EditorHost({
  session,
  entrySearch,
  onNavigate
}: {
  session: EngineSession
  entrySearch: string
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const entry = useEditorHost(entrySearch, session)
  // a travel (back to scenes) is 'entering': the page stays over it, the loading overlay yields to it
  if (entry == null || (session.phase !== 'world' && session.phase !== 'entering') || session.menuOpen) return null
  return <CreatePage entry={entry} create={session.create} profile={session.profile} onNavigate={onNavigate} />
}
