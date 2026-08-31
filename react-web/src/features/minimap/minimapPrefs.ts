// Minimap preferences, persisted across sessions in localStorage (same approach as the
// chat's emoji recents — these are page-side UI choices, not engine settings, so they don't
// belong in the `settings` bridge domain).
//
// Every read validates and falls back to a default: a stale or hand-edited value must never
// leave the minimap in a state it can't render.

import type { MinimapRotation, MinimapStyle } from '../../engine/protocol'

const STYLE_KEY = 'dcl-minimap-style'
const ROTATION_KEY = 'dcl-minimap-rotation'
const ZOOM_KEY = 'dcl-minimap-zoom'
const MARKERS_KEY = 'dcl-minimap-markers'

const STYLES: MinimapStyle[] = ['parcel', 'satellite', 'imposters']
const ROTATIONS: MinimapRotation[] = ['camera', 'north']

/** Map extent across the circle, in metres. Smaller = closer in. */
export const DEFAULT_VISIBLE_METERS = 256
export const MIN_VISIBLE_METERS = 64
export const MAX_VISIBLE_METERS = 768
export const ZOOM_STEP = 1.5

/** The place categories the places API knows, in the order the settings menu lists them. */
export const ALL_MARKER_CATEGORIES = [
  'poi',
  'featured',
  'game',
  'casino',
  'social',
  'music',
  'art',
  'fashion',
  'crypto',
  'education',
  'shop',
  'sports',
  'business',
  'parkour'
]

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
  } catch {
    // ignore quota / privacy-mode failures
  }
}

export function loadStyle(): MinimapStyle {
  const v = read(STYLE_KEY)
  return STYLES.includes(v as MinimapStyle) ? (v as MinimapStyle) : 'satellite'
}

export function saveStyle(style: MinimapStyle): void {
  write(STYLE_KEY, style)
}

export function loadRotation(): MinimapRotation {
  const v = read(ROTATION_KEY)
  return ROTATIONS.includes(v as MinimapRotation) ? (v as MinimapRotation) : 'north'
}

export function saveRotation(rotation: MinimapRotation): void {
  write(ROTATION_KEY, rotation)
}

export function loadZoom(): number {
  const v = Number(read(ZOOM_KEY))
  // Clamp on read as well as on write: the stored value outlives any change to the range.
  if (!Number.isFinite(v) || v <= 0) return DEFAULT_VISIBLE_METERS
  return Math.min(MAX_VISIBLE_METERS, Math.max(MIN_VISIBLE_METERS, v))
}

export function saveZoom(meters: number): void {
  write(ZOOM_KEY, String(meters))
}

export function loadMarkers(): string[] {
  try {
    const v: unknown = JSON.parse(read(MARKERS_KEY) ?? 'null')
    if (!Array.isArray(v)) return [...ALL_MARKER_CATEGORIES]
    return v.filter((c): c is string => typeof c === 'string')
  } catch {
    return [...ALL_MARKER_CATEGORIES]
  }
}

export function saveMarkers(categories: string[]): void {
  write(MARKERS_KEY, JSON.stringify(categories))
}
