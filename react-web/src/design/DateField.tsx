// DateField — a date entered as three of our own Selects rather than `input type="date"`.
//
// The native picker is a Chromium popup widget, and the HUD renders offscreen: CEF paints popups
// as a separate surface that the engine does not composite, so opening one replaces the whole HUD
// image with the picker's bitmap. That is the same reason Select is a custom listbox instead of a
// native `<select>` — nothing here may open a native popup.

import { useEffect, useState } from 'react'
import { Select } from './Select'
import styles from './DateField.module.css'

const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December'
]

/** Oldest year offered. Comfortably beyond any living person; the profile is not the place to
 *  enforce an age policy. */
const FIRST_YEAR = 1900

const pad = (n: number): string => String(n).padStart(2, '0')
const daysInMonth = (year: number, month: number): number =>
  year > 0 && month > 0 ? new Date(Date.UTC(year, month, 0)).getUTCDate() : 31

type Parts = { year: string; month: string; day: string }

const partsOf = (iso: string): Parts => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso)
  return m == null ? { year: '', month: '', day: '' } : { year: m[1], month: m[2], day: m[3] }
}

export interface DateFieldProps {
  /** `YYYY-MM-DD`, or '' for unset. */
  value: string
  /** The composed date, or '' while any part is still unchosen. */
  onChange: (value: string) => void
  disabled?: boolean
  variant?: 'dark' | 'light'
  /** Prefixes each part's accessible name, e.g. "Birth Date year". */
  label: string
}

export function DateField({ value, onChange, disabled, variant = 'dark', label }: DateFieldProps): React.JSX.Element {
  const [parts, setParts] = useState<Parts>(() => partsOf(value))

  // Adopt a complete date arriving from outside (the form reseeding), but never clobber a part the
  // user has chosen while the rest is still empty — `value` is '' for the whole of that.
  useEffect(() => {
    if (value !== '') setParts(partsOf(value))
  }, [value])

  const yearNum = Number(parts.year)
  const monthNum = Number(parts.month)
  const dayCount = daysInMonth(yearNum, monthNum)

  const emit = (next: Parts): void => {
    // A day that the chosen month does not have (31st of February) is pulled back to its last day,
    // rather than silently emitting a date that isn't real.
    const limit = daysInMonth(Number(next.year), Number(next.month))
    const day = next.day !== '' && Number(next.day) > limit ? pad(limit) : next.day
    const fixed = { ...next, day }
    setParts(fixed)
    onChange(fixed.year !== '' && fixed.month !== '' && fixed.day !== '' ? `${fixed.year}-${fixed.month}-${fixed.day}` : '')
  }

  const years = Array.from({ length: new Date().getUTCFullYear() - FIRST_YEAR + 1 }, (_, i) => String(new Date().getUTCFullYear() - i))

  return (
    <div className={styles.row}>
      <Select
        aria-label={`${label} day`}
        variant={variant}
        disabled={disabled}
        value={parts.day}
        options={[
          { value: '', label: 'Day' },
          ...Array.from({ length: dayCount }, (_, i) => ({ value: pad(i + 1), label: String(i + 1) }))
        ]}
        onChange={(day) => emit({ ...parts, day })}
      />
      <Select
        aria-label={`${label} month`}
        variant={variant}
        disabled={disabled}
        value={parts.month}
        options={[{ value: '', label: 'Month' }, ...MONTHS.map((m, i) => ({ value: pad(i + 1), label: m }))]}
        onChange={(month) => emit({ ...parts, month })}
      />
      <Select
        aria-label={`${label} year`}
        variant={variant}
        disabled={disabled}
        value={parts.year}
        options={[{ value: '', label: 'Year' }, ...years.map((y) => ({ value: y, label: y }))]}
        onChange={(year) => emit({ ...parts, year })}
      />
    </div>
  )
}
