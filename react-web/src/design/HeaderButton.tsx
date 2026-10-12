// HeaderButton — the dark round (or pill) buttons on a full-screen header: a 40px circle for an
// icon, or a 46px pill for a label such as the profile chip. A faint ring keeps them readable over
// a busy background.

import styles from './HeaderButton.module.css'

type Shape = 'circle' | 'pill'

export function HeaderButton({
  shape = 'circle',
  className = '',
  type = 'button',
  ...rest
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { shape?: Shape }): React.JSX.Element {
  return <button type={type} className={`${styles.btn} ${styles[shape]} ${className}`.trim()} {...rest} />
}
