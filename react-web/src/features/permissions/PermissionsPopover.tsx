// Permissions popover beside the sidebar permissions button, for the scene the player stands in:
// the player's "This visit" answers, then the stored rules for the scene, the realm and everywhere.
// A row the scene has run into this visit is highlighted — the answer or the rule in force for the
// type — so a silent refusal points at what caused it; a dismissed prompt's refusal has no row and
// shows nothing. Clicking a row clears that answer or rule at once and opens the review dialog on
// its level, so dismissing leaves it cleared. Closes on an outside click or Escape, like the
// nearby-voice popover.

import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Button, showConfirm } from '../../design'
import { registerCancelLayer } from '../../lib/cancelLayers'
import type { PermissionRuleItem, PermissionRuleLevel } from '../../engine/protocol'
import type { PermissionRules, PermissionSubject, TriggeredPermission, VisitAnswer } from '../session/useEngineSession'
import { describePermission, openPermissionReview } from './PermissionDialog'
import styles from './PermissionsPopover.module.css'

const VALUE_LABEL = { Allow: 'always', Deny: 'never', Ask: 'ask' } as const

type Level = 'visit' | 'scene' | 'realm' | 'global'

/** Which row decides `ty` for the scene: a visit answer, else the first level with a rule. */
function levelInForce(ty: string, visit: VisitAnswer[], rules: PermissionRules | null): Level | null {
  if (visit.some((v) => v.ty === ty)) return 'visit'
  if (rules == null) return null
  for (const level of ['scene', 'realm', 'global'] as const) {
    if (rules.rules[level].some((r) => r.ty === ty)) return level
  }
  return null
}

export function PermissionsPopover({
  anchor,
  visit,
  triggered,
  rules,
  sceneName,
  setRule,
  clearRule,
  resetDefaults,
  onClose
}: {
  /** The sidebar permissions button the popover sits beside. */
  anchor: HTMLElement | null
  visit: VisitAnswer[]
  triggered: TriggeredPermission[]
  rules: PermissionRules | null
  sceneName: string
  setRule: (subject: PermissionSubject, allow: boolean, level: PermissionRuleLevel) => void
  clearRule: (subject: PermissionSubject) => void
  resetDefaults: () => void
  onClose: () => void
}): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const [top, setTop] = useState<number | null>(null)
  const [left, setLeft] = useState(0)

  // Centred on the button like the voice popover, then kept on screen: the list grows with the
  // scene's answers and rules, and the panel is translateY(-50%) around `top`, so clamp its centre
  // by the measured (already scaled) height.
  useLayoutEffect(() => {
    if (anchor == null || ref.current == null) return
    const r = anchor.getBoundingClientRect()
    const half = ref.current.getBoundingClientRect().height / 2
    setTop(Math.max(half + 8, Math.min(r.top + r.height / 2, window.innerHeight - half - 8)))
    setLeft(r.left + r.width / 2 + r.width)
  }, [anchor, visit, rules, triggered])

  // pointerdown so a click on the world closes it too (the canvas suppresses mousedown)
  useEffect(() => {
    const onDown = (e: PointerEvent): void => {
      const t = e.target as Node
      if (ref.current?.contains(t) || anchor?.contains(t)) return
      onClose()
    }
    document.addEventListener('pointerdown', onDown, true)
    const off = registerCancelLayer(onClose)
    return () => {
      document.removeEventListener('pointerdown', onDown, true)
      off()
    }
  }, [anchor, onClose])

  const review = (subject: PermissionSubject, initial?: PermissionRuleLevel): void => {
    onClose()
    openPermissionReview(subject, setRule, initial)
  }
  const hit = (ty: string, level: Level): boolean => triggered.some((t) => t.ty === ty) && levelInForce(ty, visit, rules) === level
  const row = (key: string, ty: string, level: Level, value: 'Allow' | 'Deny' | 'Ask', onClick: () => void): React.JSX.Element => (
    <button key={key} type="button" className={styles.row} data-hit={hit(ty, level) || undefined} onClick={onClick}>
      {describePermission(ty)}
      <span className={styles.value} data-value={value}>
        {VALUE_LABEL[value]}
      </span>
    </button>
  )
  const ruleSection = (title: string, scope: 'scene' | 'realm' | 'global', items: PermissionRuleItem[]): React.JSX.Element | null =>
    items.length === 0 || rules == null ? null : (
      <>
        <div className={styles.section}>{title}</div>
        {items.map((r) =>
          row(`${scope}|${r.ty}`, r.ty, scope, r.allow, () => {
            const subject: PermissionSubject = { ty: r.ty, sceneName, scene: rules.scene, realm: rules.realm, scope, value: r.allow }
            clearRule(subject)
            review(subject, scope)
          })
        )}
      </>
    )
  const anyRules = rules != null && rules.rules.scene.length + rules.rules.realm.length + rules.rules.global.length > 0

  return (
    <div ref={ref} className={styles.panel} role="dialog" aria-label="Scene permissions" style={{ top: top ?? -9999, left }}>
      <div className={styles.title}>SCENE PERMISSIONS</div>
      <div className={styles.divider} />
      {visit.length === 0 && !anyRules ? (
        <div className={styles.empty}>Nothing decided for this scene yet: it asks when it needs a permission.</div>
      ) : null}
      {visit.length > 0 && (
        <>
          <div className={styles.section}>This visit</div>
          {visit.map((v) =>
            row(`visit|${v.ty}`, v.ty, 'visit', v.allowed ? 'Allow' : 'Deny', () => {
              const subject: PermissionSubject = { ty: v.ty, sceneName: v.sceneName, scene: v.scene, realm: v.realm, scope: 'visit', value: v.allowed ? 'Allow' : 'Deny' }
              clearRule(subject)
              review(subject, 'visit')
            })
          )}
        </>
      )}
      {anyRules && (
        <>
          {visit.length > 0 && <div className={styles.divider} />}
          <div className={styles.title}>SAVED ANSWERS</div>
          {ruleSection('This scene', 'scene', rules.rules.scene)}
          {ruleSection('This realm', 'realm', rules.rules.realm)}
          {ruleSection('Everywhere', 'global', rules.rules.global)}
        </>
      )}
      {(visit.length > 0 || anyRules) && (
        <div className={styles.hint}>
          Click one to reset it, then choose again or close.
          {triggered.length > 0 ? ' Highlighted: used by the scene this visit.' : ''}
        </div>
      )}
      <div className={styles.divider} />
      <Button
        variant="ghost"
        size="sm"
        className={styles.reset}
        onClick={() => {
          onClose()
          void showConfirm({
            title: 'Reset permissions?',
            body: 'Every answer you have saved is forgotten, for every scene and realm. Permissions go back to the built-in defaults.',
            confirmLabel: 'Reset'
          }).then((ok) => ok && resetDefaults())
        }}
      >
        Reset defaults
      </Button>
    </div>
  )
}
