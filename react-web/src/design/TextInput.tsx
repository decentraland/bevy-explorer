// TextInput — the plain single-line field. `SearchField` covers search (pill + magnifier);
// this is everything else: names, link titles, dates. Controlled or uncontrolled, like the
// other input primitives here.

import { useState } from 'react'
import styles from './TextInput.module.css'

export interface TextInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value' | 'size'> {
  value?: string
  defaultValue?: string
  onChange?: (value: string) => void
  /** Light = the white field used on solid panels (Settings, the passport's edit mode). */
  variant?: 'dark' | 'light'
  /** Red border + `aria-invalid`, for a value the user still has to fix. */
  invalid?: boolean
}

export function TextInput({
  value,
  defaultValue = '',
  onChange,
  variant = 'dark',
  invalid = false,
  className,
  ...rest
}: TextInputProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  return (
    <input
      {...rest}
      className={[styles.input, styles[variant], invalid ? styles.invalid : '', className ?? ''].filter(Boolean).join(' ')}
      value={v}
      aria-invalid={invalid || undefined}
      onChange={(e) => {
        if (!isControlled) setInternal(e.target.value)
        onChange?.(e.target.value)
      }}
    />
  )
}
