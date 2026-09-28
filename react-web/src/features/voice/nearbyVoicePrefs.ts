// Nearby voice preferences kept on this device: whether you hear others (off mutes incoming voice
// by zeroing the Voice volume, which comes back on re-enable).

const DISABLED_KEY = 'nearbyVoiceDisabled'
const VOLUME_KEY = 'nearbyVoiceVolume'
const USED_KEY = 'nearbyVoiceUsed'

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
