// React profile passport — the local player's card. Data comes from the bridge's
// getProfile relay (getPlayer + cached profile). Built on the design system.

import { Avatar, ControlButton } from '../../design'
import { nameColor, shortAddr, splitName } from '../../lib/identity'
import type { ProfileState } from '../session/useEngineSession'
import styles from './ProfilePanel.module.css'

export function ProfilePanel({ profile }: { profile: ProfileState }): React.JSX.Element | null {
  if (!profile.open) return null
  const p = profile.data
  const labelName = p ? (p.name.trim() ? p.name : shortAddr(p.address)) : ''
  const { base, tag } = splitName(labelName)
  const color = p ? nameColor(p.address) : 'var(--fill-4)'

  return (
    <div className={styles.root}>
      <header className={styles.head}>
        <span className={styles.title}>Profile</span>
        <ControlButton variant="solid" className={styles.closeGlyph} aria-label="Close profile" onClick={profile.toggle}>
          ×
        </ControlButton>
      </header>

      {!p ? (
        <div className={styles.empty}>Profile unavailable.</div>
      ) : (
        <div className={styles.body}>
          <Avatar src={p.picture} name={base} color={color} size={88} status="online" />
          <div className={styles.name} style={{ color }}>
            {base}
            {tag && <span className={styles.tag}>{tag}</span>}
          </div>
          <div className={styles.addr}>{shortAddr(p.address)}</div>
          {p.isGuest && <span className={styles.guest}>Guest</span>}
          {p.description && <p className={styles.desc}>{p.description}</p>}
          {p.links && p.links.length > 0 && (
            <div className={styles.links}>
              {p.links.map((l) => (
                <a key={l.url} className={styles.link} href={l.url} target="_blank" rel="noreferrer">
                  {l.title}
                </a>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
