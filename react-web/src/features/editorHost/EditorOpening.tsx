// Shown while the editor package downloads, after Create was clicked.

import { Panel, Spinner } from '../../design'
import styles from './EditorOpening.module.css'

export function EditorOpening(): React.JSX.Element {
  return (
    <Panel className={styles.root} role="status">
      <Spinner size={20} />
      Opening Create…
    </Panel>
  )
}
