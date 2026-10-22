// TextInput — the plain single-line field. `SearchField` covers search (pill + magnifier);
// this is everything else: names, link titles, dates. Controlled or uncontrolled, like the
// other input primitives here.

import { useState } from 'react'
import { Icon, type IconName } from './icons'
import styles from './TextInput.module.css'

export interface TextInputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'onChange' | 'value' | 'size'> {
  value?: string
  defaultValue?: string
  onChange?: (value: string) => void
  /** Light = the white field used on solid panels (Settings, the passport's edit mode). */
  variant?: 'dark' | 'light'
  /** lg: display-sized text, for a field that is the page's point (the guest start page's name). */
  size?: 'md' | 'lg'
  /** Red border + `aria-invalid`, for a value the user still has to fix. */
  invalid?: boolean
  /** An icon action inside the field's right edge (e.g. a dice that picks another value); `disabled`
   *  holds it while the field stays editable; `size` is the icon's, in px (18 by default): the field
   *  grows to fit a bigger one. */
  action?: { icon: IconName; label: string; onClick: () => void; disabled?: boolean; size?: number }
}

export function TextInput({
  value,
  defaultValue = '',
  onChange,
  variant = 'dark',
  size = 'md',
  invalid = false,
  action,
  className,
  ...rest
}: TextInputProps): React.JSX.Element {
  const [internal, setInternal] = useState(defaultValue)
  const isControlled = value !== undefined
  const v = isControlled ? value : internal

  const input = (
    <input
      {...rest}
      className={[styles.input, styles[variant], size === 'lg' ? styles.large : '', invalid ? styles.invalid : '', action ? styles.withAction : '', className ?? ''].filter(Boolean).join(' ')}
      value={v}
      aria-invalid={invalid || undefined}
      onChange={(e) => {
        if (!isControlled) setInternal(e.target.value)
        onChange?.(e.target.value)
      }}
    />
  )
  if (action == null) return input
  const iconSize = action.size ?? 18
  return (
    <div className={styles.wrap} style={{ '--action-icon': `${iconSize}px` } as React.CSSProperties}>
      {input}
      <button type="button" className={styles.action} aria-label={action.label} title={action.label} onClick={action.onClick} disabled={rest.disabled || action.disabled}>
        <Icon name={action.icon} size={iconSize} />
      </button>
    </div>
  )
}
