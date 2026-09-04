// The HUD session (from useEngineSession), shared via context so surfaces rendered OUTSIDE the normal
// prop-drill tree — notably popups mounted by <PopupHost/> — can read it. Hud provides it and mounts
// <PopupHost/> inside the provider; the world <ProfileCard> popup reads it with useSession().
import { createContext, useContext } from 'react'
import type { EngineSession } from './useEngineSession'

const SessionContext = createContext<EngineSession | null>(null)

export const SessionProvider = SessionContext.Provider

export function useSession(): EngineSession {
  const session = useContext(SessionContext)
  if (session == null) throw new Error('useSession must be used within a <SessionProvider>')
  return session
}
