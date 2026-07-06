// CharCounter — `current/max` readout that turns red when over the limit.
// Design references eordano/dcl-react-ui.

import styles from './CharCounter.module.css'

type CharCounterFormat = 'slash' | 'spaced' | 'words'

interface CharCounterProps extends React.HTMLAttributes<HTMLDivElement> {
  current?: number
  max?: number
  format?: CharCounterFormat
  over?: boolean
}

export function CharCounter({
  current = 0,
  max,
  format = 'slash',
  over = true,
  className = '',
  ...rest
}: CharCounterProps): React.JSX.Element {
  let text: string
  if (format === 'spaced') text = `${current} / ${max}`
  else if (format === 'words') text = `(${current} out of ${max} characters)`
  else text = `${current}/${max}`

  const isOver = over && max != null && current > max
  return (
    <div
      className={`${styles.charcounter} ${isOver ? styles.over : ''} ${className}`.trim()}
      {...rest}
    >
      {text}
    </div>
  )
}
