// React Settings — a page inside the shared MainMenuShell (consistent top bar with
// Backpack/etc). "Settings" + category sub-tabs and a two-column grid of controls
// (light Select / ruby Toggle / arrow Slider). Data + actions via the bridge relay
// of BevyApi.getSettings / setSetting.

import { memo, useMemo, useState } from 'react'
import { Select, Slider, Toggle } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { Setting } from '../../engine/protocol'
import type { ProfileState, SettingsState } from '../session/useEngineSession'
import styles from './SettingsPanel.module.css'

function humanize(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}
function isBinary(s: Setting): boolean {
  return s.namedVariants.length === 2 || (s.namedVariants.length === 0 && s.maxValue - s.minValue <= 1 && s.stepSize >= 1)
}
function isSlider(s: Setting): boolean {
  return !isBinary(s) && s.namedVariants.length <= 2
}

function Control({ s, onSet }: { s: Setting; onSet: (name: string, value: number) => void }): React.JSX.Element {
  if (isBinary(s)) {
    return <Toggle checked={s.value >= 1} onChange={(c) => onSet(s.name, c ? 1 : 0)} aria-label={s.name} />
  }
  if (s.namedVariants.length > 2) {
    return (
      <Select
        variant="light"
        value={String(s.value)}
        options={s.namedVariants.map((v, i) => ({ value: String(i), label: v.name }))}
        onChange={(v) => onSet(s.name, Number(v))}
        aria-label={s.name}
      />
    )
  }
  return (
    <Slider arrows value={s.value} min={s.minValue} max={s.maxValue} step={s.stepSize || 1} onChange={(v) => onSet(s.name, v)} aria-label={s.name} />
  )
}

// Memoised by value so a settings re-push (new array every change) only re-renders
// the one field that actually changed — keeps the menu snappy over the busy engine.
const SettingField = memo(
  function SettingField({ s, onSet }: { s: Setting; onSet: (name: string, value: number) => void }): React.JSX.Element {
    return (
      <div className={styles.field}>
        <div className={styles.fieldHead}>
          <span className={styles.label}>{humanize(s.name)}</span>
          {isSlider(s) && <span className={styles.value}>{s.value}</span>}
        </div>
        <Control s={s} onSet={onSet} />
      </div>
    )
  },
  (prev, next) => prev.s.name === next.s.name && prev.s.value === next.s.value && prev.onSet === next.onSet
)

export function SettingsPanel({
  settings,
  profile,
  onNavigate
}: {
  settings: SettingsState
  profile: ProfileState
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const categories = useMemo(() => [...new Set(settings.list.map((s) => s.category))], [settings.list])
  const [tab, setTab] = useState<string | null>(null)

  if (!settings.open) return null

  const activeTab = tab && categories.includes(tab) ? tab : categories[0]
  const items = settings.list.filter((s) => s.category === activeTab)
  const resetAll = (): void => items.forEach((s) => settings.set(s.name, s.default))

  const p = profile.data
  return (
    <MainMenuShell
      active="settings"
      profileName={p?.name}
      profilePicture={p?.picture}
      profileAddress={p?.address}
      profileClaimed={p?.hasClaimedName}
      onNavigate={onNavigate}
      onClose={settings.toggle}
    >
      <div className={styles.head}>
        <h1 className={styles.title}>Settings</h1>
        <div className={styles.tabs}>
          {categories.map((c) => (
            <button key={c} type="button" className={`${styles.tab} ${c === activeTab ? styles.tabActive : ''}`.trim()} onClick={() => setTab(c)}>
              {humanize(c)}
            </button>
          ))}
        </div>
        <button type="button" className={styles.reset} onClick={resetAll}>
          ↺ Reset all defaults
        </button>
      </div>

      <div className={styles.card}>
        {items.length === 0 ? (
          <div className={styles.empty}>No settings available.</div>
        ) : (
          <div className={styles.grid}>
            {items.map((s) => (
              <SettingField key={s.name} s={s} onSet={settings.set} />
            ))}
          </div>
        )}
      </div>
    </MainMenuShell>
  )
}
