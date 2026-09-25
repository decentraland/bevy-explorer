// ColorPicker — Unity's ColorPickerView: preset swatches plus hue / saturation / brightness sliders.
// Used by the Backpack for skin, hair and eye colors.

import { hexToHsv, hsvToHex } from '../lib/color'
import { Slider } from './Slider'
import styles from './ColorPicker.module.css'

export function ColorPicker({
  label,
  value,
  presets,
  onChange
}: {
  label: string
  value: string
  presets: readonly string[]
  onChange: (hex: string) => void
}): React.JSX.Element {
  const hsv = hexToHsv(value)
  const current = value.toLowerCase()
  return (
    <div className={styles.picker} role="group" aria-label={label}>
      <div className={styles.swatches} role="radiogroup" aria-label={`${label} presets`}>
        {presets.map((p) => (
          <button
            key={p}
            type="button"
            role="radio"
            aria-label={p}
            aria-checked={p.toLowerCase() === current}
            className={styles.swatch}
            style={{ background: p }}
            onClick={() => onChange(p)}
          />
        ))}
      </div>
      <label className={styles.row}>
        <span>Hue</span>
        <Slider aria-label="Hue" value={Math.round(hsv.h)} min={0} max={359} onChange={(h) => onChange(hsvToHex({ ...hsv, h }))} />
      </label>
      <label className={styles.row}>
        <span>Saturation</span>
        <Slider aria-label="Saturation" value={Math.round(hsv.s * 100)} onChange={(s) => onChange(hsvToHex({ ...hsv, s: s / 100 }))} />
      </label>
      <label className={styles.row}>
        <span>Brightness</span>
        <Slider aria-label="Brightness" value={Math.round(hsv.v * 100)} onChange={(v) => onChange(hsvToHex({ ...hsv, v: v / 100 }))} />
      </label>
    </div>
  )
}
