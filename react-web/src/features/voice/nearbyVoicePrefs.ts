// Nearby voice preferences kept on this device: whether you hear others (off mutes incoming voice
// by zeroing the Voice volume, which comes back on re-enable) and the intro tip's schedule.

import { launchCount } from '../../lib/launchCount'

const DISABLED_KEY = 'nearbyVoiceDisabled'
const VOLUME_KEY = 'nearbyVoiceVolume'
const TIP_SHOWN_KEY = 'nearbyVoiceTipShown'
const TIP_LAST_KEY = 'nearbyVoiceTipLastLaunch'
const USED_KEY = 'nearbyVoiceUsed'

// Shown every 5 launches, at most twice, never once the player has talked.
const TIP_EVERY = 5
const TIP_MAX = 2

function read(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {}
}

export function hearOthers(): boolean {
  return read(DISABLED_KEY) !== 'true'
}

export function setHearOthers(on: boolean): void {
  write(DISABLED_KEY, String(!on))
}

/** The Voice volume to restore when hearing others again. */
export function savedVolume(): number {
  const v = Number(read(VOLUME_KEY))
  return Number.isFinite(v) && v > 0 ? v : 100
}

export function saveVolume(volume: number): void {
  write(VOLUME_KEY, String(volume))
}

export function markVoiceUsed(): void {
  write(USED_KEY, 'true')
}

export function tipDue(): boolean {
  const launches = launchCount()
  const shown = Number(read(TIP_SHOWN_KEY)) || 0
  const last = Number(read(TIP_LAST_KEY)) || 0
  return read(USED_KEY) !== 'true' && shown < TIP_MAX && launches >= TIP_EVERY && launches - last >= TIP_EVERY
}

export function markTipShown(): void {
  write(TIP_SHOWN_KEY, String((Number(read(TIP_SHOWN_KEY)) || 0) + 1))
  write(TIP_LAST_KEY, String(launchCount()))
}
