// How many times this device has launched the explorer; the loading screen picks tips for new or
// returning players from it.

const KEY = 'launchCount'

let count = 0

export function countLaunch(): void {
  try {
    count = (Number(localStorage.getItem(KEY)) || 0) + 1
    localStorage.setItem(KEY, String(count))
  } catch {
    count = 1
  }
}

export function launchCount(): number {
  return count
}
