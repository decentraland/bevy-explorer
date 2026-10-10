// React Settings — a page inside the shared MainMenuShell (consistent top bar with
// Backpack/etc). "Settings" + category sub-tabs and a two-column grid of controls
// (light Select / ruby Toggle / arrow Slider). Data + actions via the bridge relay
// of BevyApi.getSettings / setSetting.

import { memo, useMemo, useState } from 'react'
import { Select, Slider, Tabs, Toggle, showConfirm } from '../../design'
import { MainMenuShell } from '../menu/MainMenuShell'
import type { Setting } from '../../engine/protocol'
import type { BindingsState, ProfileState, SettingsState } from '../session/useEngineSession'
import { KeyBindingsTab } from './KeyBindingsTab'
import {
  LANGUAGES,
  clearAutoTranslate,
  nativeName,
  setAutoTranslateDefault,
  setTranslationLanguage,
  systemLanguage,
  useTranslationPrefs
} from '../chat/translation'
import styles from './SettingsPanel.module.css'

// Appended after the engine-derived categories: bindings are (action, keys[]) rows from the
// bindings relay, not numeric engine settings, so they get their own tab + body.
const KEY_BINDINGS_TAB = 'Key Bindings'
// The HUD's own chat settings (translation) join the engine's in this tab.
const CHAT_TAB = 'Chat'

function humanize(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}
// A toggle only reads right for an off/on pair; any other pair of names (Everyone / Only friends)
// needs its names on screen, so it is a select like the longer lists.
function isBinary(s: Setting): boolean {
  if (s.namedVariants.length === 2) return s.namedVariants.every((v) => /^(off|on)$/i.test(v.name))
  return s.namedVariants.length === 0 && s.maxValue - s.minValue <= 1 && s.stepSize >= 1
}
function isSlider(s: Setting): boolean {
  return !isBinary(s) && s.namedVariants.length < 2
}

function Control({ s, onSet }: { s: Setting; onSet: (name: string, value: number) => void }): React.JSX.Element {
  // A negative value is the engine saying the setting is not known yet (e.g. a server-owned
  // one before the service answers): shown, but not editable.
  const unset = s.value < 0
  if (isBinary(s)) {
    return <Toggle checked={s.value >= 1} disabled={unset} onChange={(c) => onSet(s.name, c ? 1 : 0)} aria-label={s.name} />
  }
  if (s.namedVariants.length >= 2) {
    return (
      <Select
        variant="light"
        value={unset ? '' : String(s.value)}
        disabled={unset}
        options={s.namedVariants.map((v, i) => ({ value: String(i), label: v.name }))}
        onChange={(v) => onSet(s.name, Number(v))}
        aria-label={s.name}
      />
    )
  }
  return (
    <Slider arrows value={s.value} min={s.minValue} max={s.maxValue} step={s.stepSize || 1} disabled={unset} onChange={(v) => onSet(s.name, v)} aria-label={s.name} />
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

/** Chat translation, kept by the HUD rather than the engine. */
function TranslationFields(): React.JSX.Element {
  const prefs = useTranslationPrefs()
  const system = systemLanguage()
  // each in its own name, so a player finds theirs without reading English
  const options = useMemo(
    () => LANGUAGES.map((l) => ({ value: l.code, label: nativeName(l.code) })).sort((a, b) => a.label.localeCompare(b.label)),
    []
  )
  return (
    <>
      <div className={styles.field}>
        <div className={styles.fieldHead}>
          <span className={styles.label}>Translation Language</span>
        </div>
        <Select
          variant="light"
          value={prefs.languageChosen ? prefs.language : ''}
          options={[{ value: '', label: `System (${nativeName(system)})` }, ...options]}
          onChange={(v) => setTranslationLanguage(v === '' ? null : v)}
          aria-label="Translation Language"
        />
      </div>
      <div className={styles.field}>
        <div className={styles.fieldHead}>
          <span className={styles.label}>Auto-Translate New Conversations</span>
        </div>
        <Toggle checked={prefs.autoDefault} onChange={setAutoTranslateDefault} aria-label="Auto-Translate New Conversations" />
      </div>
    </>
  )
}

export function SettingsPanel({
  settings,
  bindings,
  profile,
  onNavigate
}: {
  settings: SettingsState
  bindings: BindingsState
  profile: ProfileState
  onNavigate: (page: string) => void
}): React.JSX.Element | null {
  const categories = useMemo(
    () => [...new Set([...settings.list.map((s) => s.category), CHAT_TAB]), KEY_BINDINGS_TAB],
    [settings.list]
  )
  const [tab, setTab] = useState<string | null>(null)

  if (!settings.open) return null

  const activeTab = tab && categories.includes(tab) ? tab : categories[0]
  const items = settings.list.filter((s) => s.category === activeTab)
  const resetAll = (): void => {
    if (activeTab === KEY_BINDINGS_TAB) {
      void showConfirm({
        title: 'Reset key bindings?',
        body: 'All bindings return to their defaults. This cannot be undone.',
        confirmLabel: 'Reset'
      }).then((ok) => ok && bindings.reset())
      return
    }
    items.forEach((s) => settings.set(s.name, s.default))
    if (activeTab === CHAT_TAB) {
      setTranslationLanguage(null)
      setAutoTranslateDefault(false)
      clearAutoTranslate()
    }
  }

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
        <Tabs items={categories.map((c) => ({ id: c, label: humanize(c) }))} value={activeTab} onChange={setTab} aria-label="Settings categories" />
        <button type="button" className={styles.reset} onClick={resetAll}>
          ↺ Reset all defaults
        </button>
      </div>

      <div className={styles.card}>
        {activeTab === KEY_BINDINGS_TAB ? (
          <KeyBindingsTab bindings={bindings} />
        ) : items.length === 0 && activeTab !== CHAT_TAB ? (
          <div className={styles.empty}>No settings available.</div>
        ) : (
          <div className={styles.grid}>
            {items.map((s) => (
              <SettingField key={s.name} s={s} onSet={settings.set} />
            ))}
            {activeTab === CHAT_TAB && <TranslationFields />}
          </div>
        )}
      </div>
    </MainMenuShell>
  )
}
