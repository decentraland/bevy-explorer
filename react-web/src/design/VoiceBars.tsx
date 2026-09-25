// VoiceBars — Unity's nametag speaking badge (NametagStyle.uss __badge-voice-chat): three green
// bars that bounce while someone talks.

import styles from './VoiceBars.module.css'

export function VoiceBars({ label = 'Speaking' }: { label?: string }): React.JSX.Element {
  return (
    <span className={styles.bars} role="img" aria-label={label} data-speaking-bars>
      <span className={styles.bar} />
      <span className={`${styles.bar} ${styles.middle}`} />
      <span className={styles.bar} />
    </span>
  )
}
