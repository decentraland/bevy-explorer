// Nearby voice preferences kept on this device: whether you hear others (off mutes incoming voice
// by zeroing the Voice volume, which comes back on re-enable).

import { getPref, PREF, setPref } from '../../lib/prefs'

const DISABLED_KEY = PREF.voiceDisabled
const VOLUME_KEY = PREF.voiceVolume
const USED_KEY = PREF.voiceUsed

const read = getPref
const write = setPref

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
