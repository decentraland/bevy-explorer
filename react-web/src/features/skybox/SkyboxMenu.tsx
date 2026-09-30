// Skybox menu — Unity's sidebar SkyboxMenu (DCL/UI/Skybox): a time-of-day slider, locked while
// "Time progression" lets the engine's day cycle run.

import { Panel, Slider, Toggle } from '../../design'
import type { SkyboxState } from '../session/useEngineSession'
import styles from './SkyboxMenu.module.css'

export function formatHours(hours: number): string {
  const total = Math.round(hours * 60) % (24 * 60)
  return `${String(Math.floor(total / 60)).padStart(2, '0')}:${String(total % 60).padStart(2, '0')}`
}

export function SkyboxMenu({ skybox }: { skybox: SkyboxState }): React.JSX.Element | null {
  if (!skybox.open) return null
  return (
    <Panel className={styles.root} role="dialog" aria-label="Skybox">
      <div className={styles.head}>
        <span className={styles.heading}>Skybox</span>
        <span className={styles.time}>{formatHours(skybox.hours)}</span>
      </div>
      <Slider value={skybox.hours} min={0} max={24} step={0.25} onChange={skybox.setHours} disabled={skybox.progressing} />
      <label className={styles.row}>
        <span>Time progression</span>
        <Toggle checked={skybox.progressing} onChange={skybox.setProgressing} />
      </label>
    </Panel>
  )
}
