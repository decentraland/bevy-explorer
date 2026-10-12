// ColorPicker — a COLOR button showing the current
// color that opens a light popup with preset toggles and hue / saturation / brightness bars.

import { useEffect, useRef, useState } from 'react'
import { registerCancelLayer } from '../lib/cancelLayers'
import { hexToHsv, hsvToHex, type Hsv } from '../lib/color'
import styles from './ColorPicker.module.css'

// Arrow step; the bars run 0–1.
const STEP = 0.1

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
  const [open, setOpen] = useState(false)
  // The bars hold their own values, so hue survives zero saturation or brightness.
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(value))
  const [preset, setPreset] = useState<string | null>(null)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onDown = (e: MouseEvent): void => {
      if (ref.current != null && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', onDown)
    return () => document.removeEventListener('mousedown', onDown)
  }, [open])

  // A cancel layer, so Cancel (Escape or gamepad) closes just the popup, not the Backpack.
  useEffect(() => {
    if (!open) return
    return registerCancelLayer(() => setOpen(false))
  }, [open])

  const toggle = (): void => {
    if (!open) {
      setHsv(hexToHsv(value))
      setPreset(presets.find((p) => p.toLowerCase() === value.toLowerCase()) ?? null)
    }
    setOpen(!open)
  }
  const pickPreset = (p: string): void => {
    setHsv(hexToHsv(p))
    setPreset(p)
    onChange(p)
  }
  const setChannel = (k: keyof Hsv, v: number): void => {
    const next = { ...hsv, [k]: k === 'h' ? Math.min(359.999, Math.max(0, v * 360)) : Math.min(1, Math.max(0, v)) }
    setHsv(next)
    setPreset(null)
    onChange(hsvToHex(next))
  }
  const hueHex = hsvToHex({ h: hsv.h, s: 1, v: 1 })

  return (
    <div className={styles.root} ref={ref}>
      <button type="button" className={styles.trigger} aria-label={label} aria-expanded={open} onClick={toggle}>
        <span className={styles.triggerText}>COLOR</span>
        <span className={styles.preview} style={{ background: value }} />
        <span className={`${styles.chevron} ${open ? styles.chevronUp : ''}`.trim()} aria-hidden="true" />
      </button>
      {open && (
        <div className={styles.popup} role="dialog" aria-label={label}>
          <div className={styles.section}>
            <span className={styles.label}>PRESETS</span>
            <div className={styles.presets} role="radiogroup" aria-label={`${label} presets`}>
              {presets.map((p) => (
                <button
                  key={p}
                  type="button"
                  role="radio"
                  aria-label={p}
                  aria-checked={p === preset}
                  className={styles.preset}
                  onClick={() => pickPreset(p)}
                >
                  <span className={styles.tint} style={{ background: p }} />
                </button>
              ))}
            </div>
          </div>
          <Bar label="COLOR" name="Hue" value={hsv.h / 360} track={styles.hueTrack} onChange={(v) => setChannel('h', v)} />
          <Bar
            label="SATURATION"
            name="Saturation"
            value={hsv.s}
            track={styles.satTrack}
            trackStyle={{ '--hue': hueHex } as React.CSSProperties}
            onChange={(v) => setChannel('s', v)}
          />
          <Bar label="BRIGHTNESS" name="Brightness" value={hsv.v} track={styles.valTrack} onChange={(v) => setChannel('v', v)} />
        </div>
      )}
    </div>
  )
}

function Bar({
  label,
  name,
  value,
  track,
  trackStyle,
  onChange
}: {
  label: string
  name: string
  value: number
  track: string
  trackStyle?: React.CSSProperties
  onChange: (v: number) => void
}): React.JSX.Element {
  return (
    <div className={styles.section}>
      <span className={styles.label}>{label}</span>
      <div className={styles.barRow}>
        <button type="button" className={styles.step} aria-label={`Decrease ${name}`} disabled={value <= 0} onClick={() => onChange(value - STEP)}>
          ‹
        </button>
        <div className={`${styles.track} ${track}`} style={trackStyle}>
          <input
            className={styles.range}
            type="range"
            min={0}
            max={1}
            step={0.001}
            aria-label={name}
            value={value}
            onChange={(e) => onChange(Number(e.target.value))}
          />
        </div>
        <button type="button" className={styles.step} aria-label={`Increase ${name}`} disabled={value >= 0.999} onClick={() => onChange(value + STEP)}>
          ›
        </button>
      </div>
    </div>
  )
}
