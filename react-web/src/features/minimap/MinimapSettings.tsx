// The minimap's gear menu: rotation mode, visualisation style, and which place categories get
// markers — the same three sections the SDK7 HUD's minimap settings had.
//
// Markers are modelled as the full category list even though the menu only offers three
// presets, so per-category toggles stay a UI change rather than a data change.

import { Check } from '../../design'
import { ALL_MARKER_CATEGORIES } from './minimapPrefs'
import type { MinimapRotation, MinimapStyle } from '../../engine/protocol'
import styles from './MinimapSettings.module.css'

const ROTATION_LABELS: Array<{ value: MinimapRotation; label: string }> = [
  { value: 'camera', label: 'Rotate with camera' },
  { value: 'north', label: 'Fixed north' }
]

const STYLE_LABELS: Array<{ value: MinimapStyle; label: string }> = [
  { value: 'parcel', label: 'Parcel atlas' },
  { value: 'satellite', label: 'Satellite' },
  { value: 'imposters', label: 'Camera' }
]

type MarkerPreset = 'all' | 'poi' | 'none'

function presetOf(markers: string[]): MarkerPreset | null {
  if (markers.length === 0) return 'none'
  if (markers.length === 1 && markers[0] === 'poi') return 'poi'
  if (markers.length === ALL_MARKER_CATEGORIES.length) return 'all'
  return null
}

const MARKER_PRESETS: Array<{ value: MarkerPreset; label: string; categories: string[] }> = [
  { value: 'all', label: 'All', categories: ALL_MARKER_CATEGORIES },
  { value: 'poi', label: 'Only POI', categories: ['poi'] },
  { value: 'none', label: 'None', categories: [] }
]

export function MinimapSettings({
  rotation,
  onRotation,
  style,
  onStyle,
  hideStyle,
  markers,
  onMarkers
}: {
  rotation: MinimapRotation
  onRotation: (r: MinimapRotation) => void
  style: MinimapStyle
  onStyle: (s: MinimapStyle) => void
  /** True in a World, where only the Camera style can render anything. */
  hideStyle: boolean
  markers: string[]
  onMarkers: (categories: string[]) => void
}): React.JSX.Element {
  const preset = presetOf(markers)

  return (
    <div className={styles.menu} role="menu" aria-label="Minimap settings">
      <p className={styles.section}>Rotation mode</p>
      {ROTATION_LABELS.map((o) => (
        <Option key={o.value} label={o.label} selected={rotation === o.value} onSelect={() => onRotation(o.value)} />
      ))}

      {!hideStyle && (
        <>
          <p className={styles.section}>Visualization style</p>
          {STYLE_LABELS.map((o) => (
            <Option key={o.value} label={o.label} selected={style === o.value} onSelect={() => onStyle(o.value)} />
          ))}
        </>
      )}

      <p className={styles.section}>Markers</p>
      {MARKER_PRESETS.map((o) => (
        <Option
          key={o.value}
          label={o.label}
          selected={preset === o.value}
          onSelect={() => onMarkers([...o.categories])}
        />
      ))}
    </div>
  )
}

function Option({ label, selected, onSelect }: { label: string; selected: boolean; onSelect: () => void }): React.JSX.Element {
  return (
    <button type="button" className={styles.option} role="menuitemradio" aria-checked={selected} onClick={onSelect}>
      <span className={styles.check}>{selected && <Check size={13} />}</span>
      {label}
    </button>
  )
}
