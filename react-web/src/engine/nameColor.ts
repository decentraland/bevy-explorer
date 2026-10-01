// Shared by the HUD and the bridge's nametags, so a name has one colour everywhere. No DOM imports.

/** The reference client's name colour: an FNV-1a hue of the display name (letters and digits only,
 *  plus #last4 of the address when unclaimed) at 75% saturation, full value. */
export function userNameColor(name: string, address: string, claimed: boolean): string {
  const base = [...name.split('#')[0]].filter((c) => /[\p{L}\p{N}]/u.test(c)).join('')
  if (base === '') return '#ffffff'
  const display = claimed || address.length <= 4 ? base : `${base}#${address.slice(-4)}`
  let h = 2166136261
  for (let i = 0; i < display.length; i++) {
    h ^= display.charCodeAt(i)
    h = Math.imul(h, 16777619) >>> 0
  }
  const hue = h / 4294967295
  const i = Math.floor(hue * 6)
  const f = hue * 6 - i
  const [p, q, t] = [0.25, 1 - f * 0.75, 1 - (1 - f) * 0.75]
  const rgb = [[1, t, p], [q, 1, p], [p, 1, t], [p, q, 1], [t, p, 1], [1, p, q]][i % 6]
  return `#${rgb.map((x) => Math.round(x * 255).toString(16).padStart(2, '0')).join('')}`
}

export interface Rgb {
  r: number
  g: number
  b: number
}

/** A claimed name's chosen colour wins; otherwise the name-derived hue. */
export function resolveNameColor(name: string, address: string, claimed: boolean, custom?: Rgb | null): string {
  if (claimed && custom != null) {
    const hex = (v: number): string => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, '0')
    return `#${hex(custom.r)}${hex(custom.g)}${hex(custom.b)}`
  }
  return userNameColor(name, address, claimed)
}
