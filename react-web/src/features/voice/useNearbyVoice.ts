// Nearby voice state for the sidebar button and its popover: hearing others (the engine's Voice
// volume, zeroed while off), the mic, and whether we're being heard right now.

import { useCallback, useEffect, useState } from 'react'
import type { EngineSession } from '../session/useEngineSession'
import { hearOthers, markVoiceUsed, savedVolume, saveVolume, setHearOthers } from './nearbyVoicePrefs'

const VOICE_SETTING = 'Voice'

export interface NearbyVoice {
  hearing: boolean
  setHearing: (on: boolean) => void
  volume: number
  setVolume: (volume: number) => void
  speaking: boolean
  /** The local player's voice is being picked up right now. */
  talking: boolean
  micAvailable: boolean
  toggleSpeak: () => void
}

export function useNearbyVoice(session: EngineSession): NearbyVoice {
  const [hearing, setHearingState] = useState(hearOthers)
  const { load, set } = session.settings
  useEffect(() => load(), [load])
  const volume = session.settings.list.find((s) => s.name === VOICE_SETTING)?.value ?? savedVolume()
  const me = session.profile.data?.address.toLowerCase()
  const talking = me != null && session.chat.speaking.has(me)

  const setVolume = useCallback((v: number) => {
    set(VOICE_SETTING, v)
    if (v > 0) saveVolume(v)
  }, [set])

  const { enabled, toggle } = session.mic
  const setHearing = useCallback((on: boolean) => {
    setHearOthers(on)
    setHearingState(on)
    if (on) {
      set(VOICE_SETTING, savedVolume())
      return
    }
    if (volume > 0) saveVolume(volume)
    set(VOICE_SETTING, 0)
    if (enabled) toggle()
  }, [set, volume, enabled, toggle])

  const toggleSpeak = useCallback(() => {
    if (!enabled) markVoiceUsed()
    toggle()
  }, [enabled, toggle])

  return { hearing, setHearing, volume, setVolume, speaking: enabled, talking, micAvailable: session.mic.available, toggleSpeak }
}
