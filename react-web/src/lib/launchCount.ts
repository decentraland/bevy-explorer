// How many times this device has launched the explorer; the loading screen picks tips for new or
// returning players from it.

import { getPref, PREF, setPref } from './prefs'

let count = 0

export function countLaunch(): void {
  count = (Number(getPref(PREF.launchCount)) || 0) + 1
  setPref(PREF.launchCount, String(count))
}

export function launchCount(): number {
  return count
}
