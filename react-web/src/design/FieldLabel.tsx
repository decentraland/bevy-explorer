// FieldLabel — form label with an optional `notice` superscript and `sublabel`
// helper line. Design references eordano/dcl-react-ui.

import styles from './FieldLabel.module.css'

interface FieldLabelProps extends React.LabelHTMLAttributes<HTMLLabelElement> {
  children?: React.ReactNode
  htmlFor?: string
  sublabel?: React.ReactNode
  notice?: React.ReactNode
}

export function FieldLabel({
  children,
  htmlFor,
  sublabel,
  notice,
  className = '',
  ...rest
}: FieldLabelProps): React.JSX.Element {
  const label = (
    <label htmlFor={htmlFor} className={`${styles.fieldlabel} ${className}`.trim()} {...rest}>
      {children}
      {notice != null && <sup className={styles.notice}>{notice}</sup>}
    </label>
  )

  if (sublabel == null) return label
  return (
    <span className={styles.group}>
      {label}
      <span className={styles.sub}>{sublabel}</span>
    </span>
  )
}
