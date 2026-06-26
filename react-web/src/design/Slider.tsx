// Slider — styled range input with a ruby fill + white knob (DCL settings style).

import styles from './Slider.module.css'

interface SliderProps {
  value: number
  min?: number
  max?: number
  step?: number
  onChange: (value: number) => void
  disabled?: boolean
  /** Show ‹ › stepper arrows (DCL settings style). */
  arrows?: boolean
  'aria-label'?: string
}

export function Slider({
  value,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  disabled = false,
  arrows = false,
  'aria-label': ariaLabel
}: SliderProps): React.JSX.Element {
  const pct = max > min ? ((value - min) / (max - min)) * 100 : 0
  const clamp = (v: number): number => Math.min(max, Math.max(min, v))
  const input = (
    <input
      type="range"
      className={styles.slider}
      value={value}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
      aria-label={ariaLabel}
      style={{ '--pct': `${pct}%` } as React.CSSProperties}
      onChange={(e) => onChange(Number(e.target.value))}
    />
  )
  if (!arrows) return input
  return (
    <div className={styles.row}>
      <button type="button" className={styles.arrow} disabled={disabled || value <= min} aria-label="decrease" onClick={() => onChange(clamp(value - step))}>
        ‹
      </button>
      {input}
      <button type="button" className={styles.arrow} disabled={disabled || value >= max} aria-label="increase" onClick={() => onChange(clamp(value + step))}>
        ›
      </button>
    </div>
  )
}
