// SearchField — pill input with a leading magnifier. Controlled (`value`) or
// uncontrolled (`defaultValue`); design references eordano/dcl-react-ui.

import { useState } from 'react'
import styles from './SearchField.module.css'

interface SearchFieldProps {
  /** 'light': off-white field with dark ink and a clear button (menu pages). */
  variant?: 'dark' | 'light'
  value?: string
  defaultValue?: string
  placeholder?: string
  onChange?: (value: string) => void
}

export function SearchField({
  variant = 'dark',
  value,
  defaultValue = '',
  placeholder = 'Search',
  onChange
}: SearchFieldProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  const setValue = (next: string): void => {
    if (!isControlled) setInternal(next)
    onChange?.(next)
  }
  const set = (e: React.ChangeEvent<HTMLInputElement>): void => setValue(e.target.value)

  return (
    <label className={`${styles.search} ${variant === 'light' ? styles.light : ''}`.trim()}>
      <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" className={styles.icon}>
        <circle cx="7" cy="7" r="5" fill="none" stroke="currentColor" strokeWidth="1.6" />
        <path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
      <input
        className={styles.input}
        type="text"
        aria-label={placeholder}
        placeholder={placeholder}
        value={v}
        onChange={set}
      />
      {variant === 'light' && v !== '' && (
        <button type="button" className={styles.clear} aria-label="Clear search" onClick={() => setValue('')}>
          <span className={styles.clearGlyph} aria-hidden="true" />
        </button>
      )}
    </label>
  )
}
