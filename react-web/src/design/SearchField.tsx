// SearchField — pill input with a leading magnifier. Controlled (`value`) or
// uncontrolled (`defaultValue`); design references eordano/dcl-react-ui.

import { useState } from 'react'
import styles from './SearchField.module.css'

interface SearchFieldProps {
  value?: string
  defaultValue?: string
  placeholder?: string
  onChange?: (value: string) => void
}

export function SearchField({
  value,
  defaultValue = '',
  placeholder = 'Search',
  onChange
}: SearchFieldProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  const set = (e: React.ChangeEvent<HTMLInputElement>): void => {
    const next = e.target.value
    if (!isControlled) setInternal(next)
    onChange?.(next)
  }

  return (
    <label className={styles.search}>
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
    </label>
  )
}
