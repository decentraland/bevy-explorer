// TextArea — the multi-line field (the passport's About Me). Pairs with CharCounter when a
// `maxLength` is set: the counter is the only warning a user gets before the browser silently
// stops accepting characters.

import { useState } from 'react'
import { CharCounter } from './CharCounter'
import styles from './TextArea.module.css'

export interface TextAreaProps extends Omit<React.TextareaHTMLAttributes<HTMLTextAreaElement>, 'onChange' | 'value'> {
  value?: string
  defaultValue?: string
  onChange?: (value: string) => void
  variant?: 'dark' | 'light'
  /** Show a `current/max` counter under the field. Needs `maxLength` to have a max to count to. */
  counter?: boolean
}

export function TextArea({
  value,
  defaultValue = '',
  onChange,
  variant = 'dark',
  counter = false,
  className,
  rows = 4,
  ...rest
}: TextAreaProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  return (
    <div className={styles.wrap}>
      <textarea
        {...rest}
        rows={rows}
        className={[styles.field, styles[variant], className ?? ''].filter(Boolean).join(' ')}
        value={v}
        onChange={(e) => {
          if (!isControlled) setInternal(e.target.value)
          onChange?.(e.target.value)
        }}
      />
      {counter && rest.maxLength != null && <CharCounter className={styles.counter} current={v.length} max={rest.maxLength} />}
    </div>
  )
}
