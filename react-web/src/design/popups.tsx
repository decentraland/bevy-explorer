import { useEffect, useRef, useSyncExternalStore, type ReactNode } from 'react'
import { ModalShell } from './Modal'
import { Button } from './Button'
import { useFocusTrap } from '../lib/useFocusTrap'
import styles from './popups.module.css'

/** A popup is a render function given its own `close` callback; it returns the overlay to render. */
type PopupRender = (close: () => void) => ReactNode

/** Per-popup options. By default the popup layer draws a full-screen, dimmed+blurred backdrop that
 *  closes the popup when clicked. Set `dim: false` for an anchored popover (transparent click-catcher,
 *  e.g. the profile card); set `backdrop: false` for content that owns its own scrim (dialogs). */
export interface PopupOptions {
  backdrop?: boolean
  /** The backdrop is the shared dimmed+blurred modal scrim (default). `false` → transparent
   *  click-catcher, for an anchored popover that must not dim the HUD behind it. */
  dim?: boolean
  backdropClickCloses?: boolean
  /** Dismiss contract: run once when the popup leaves the stack by ANY path — backdrop click, the
   *  returned handle, or the central Escape. Owners that hold state behind the popup settle it here
   *  (e.g. showDialog resolves its promise), so a keyboard/Escape close never leaks. */
  onClose?: () => void
}
type ResolvedOptions = Required<Omit<PopupOptions, 'onClose'>> & Pick<PopupOptions, 'onClose'>
const DEFAULTS: Required<Omit<PopupOptions, 'onClose'>> = { backdrop: true, dim: true, backdropClickCloses: true }
type PopupNode = { id: number; render: PopupRender; options: ResolvedOptions }

// Module-level popup stack — a single HUD-wide layer (like the hoverPos store), NOT React state.
// Plain functions mutate it and notify subscribers, so a popup can be opened from anywhere (a
// component, an event handler, a util) without prop-threading or a hook — and a popup can open
// another (community → passport → confirm). <PopupHost/>, mounted once, subscribes and renders it.
let stack: PopupNode[] = []
let nextId = 0
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())
// Remove the node first, then run its onClose, so every close path is idempotent: a re-entrant close
// (an owner whose onClose fires its own handle) finds no node and stops here.
const closeById = (id: number): void => {
  const node = stack.find((n) => n.id === id)
  if (!node) return
  stack = stack.filter((n) => n.id !== id)
  emit()
  node.options.onClose?.()
}

/** Close the topmost popup (no-op if the stack is empty). Fired by PopupHost's own Escape handler
 *  (below) and, as a backup while the engine holds keyboard focus, by the engine's 'Cancel' system
 *  action relayed through the bridge — see useEngineSession. Closes one layer at a time, so stacked
 *  popups dismiss in order. */
export function closeTopPopup(): void {
  if (stack.length > 0) closeById(stack[stack.length - 1].id)
}

/** Is any popup on the stack? Read at call time (the stack changes without a React render) by the
 *  HUD's Enter/"Chat" action, which must not focus the chat sitting behind a popup — see
 *  useEngineSession. */
export function hasOpenPopup(): boolean {
  return stack.length > 0
}

/** Open an arbitrary popup imperatively; returns a `close` handle. Callable from anywhere. */
export function openPopup(render: PopupRender, options?: PopupOptions): () => void {
  const id = ++nextId
  stack = [...stack, { id, render, options: { ...DEFAULTS, ...options } }]
  emit()
  return () => closeById(id)
}

/** Hard-clear the popup stack, skipping the `onClose` contract — for tests only (the store is a
 *  module singleton, so it leaks across tests). */
export function resetPopups(): void {
  stack = []
  emit()
}

// getSnapshot must return a stable reference between changes (updates replace `stack` immutably),
// or useSyncExternalStore loops. `subscribe`/`getSnapshot` are module-stable, so no useCallback.
const subscribe = (cb: () => void): (() => void) => {
  listeners.add(cb)
  return () => listeners.delete(cb)
}
const getSnapshot = (): typeof stack => stack

/** One rendered popup layer. The topmost popup with a backdrop owns the focus trap: it focuses itself
 *  on open, cycles Tab/Shift+Tab within its content, and restores focus to the opener on close — so no
 *  popup needs its own trap. A `backdrop:false` popup (a dialog on Modal) renders bare and keeps
 *  managing its own focus until it moves onto the shared scrim. */
function PopupLayer({ node, isTop }: { node: PopupNode; isTop: boolean }): React.JSX.Element {
  const ref = useRef<HTMLDivElement>(null)
  const close = (): void => closeById(node.id)
  const content = node.render(close)

  // Only the top backdrop popup traps focus (bare content, if any, has no ref → the hook no-ops).
  useFocusTrap(ref, isTop)

  // No backdrop → the content owns its own scrim (dialogs). Otherwise the popup layer draws it:
  // `.dim` is the shared dimmed+blurred modal scrim; without `dim` it's a transparent click-catcher
  // for an anchored popover (the profile card).
  if (!node.options.backdrop) return <>{content}</>
  const className = node.options.dim ? `${styles.backdrop} ${styles.dim}` : styles.backdrop
  return (
    <div ref={ref} className={className} tabIndex={-1} onClick={node.options.backdropClickCloses ? close : undefined}>
      {/* dim popups scale in via the pop layer; an anchored popover (dim:false) just appears. */}
      {node.options.dim ? <div className={styles.pop}>{content}</div> : content}
    </div>
  )
}

/** Mounted once at the HUD root (see App) — the single React subscriber that renders the popup stack.
 *  It has no transformed ancestor, so a popup's own `position: fixed` resolves against the viewport;
 *  no portal is needed (the passport / dialogs already rely on that for their inline scrims). */
export function PopupHost(): React.JSX.Element {
  const snap = useSyncExternalStore(subscribe, getSnapshot)
  // The single, DOM-level Escape handler for every popup — so no popup needs its own. Capture phase
  // + stopPropagation so it wins over (and suppresses) the engine's Cancel relay and Modal's own key
  // handler, closing exactly one layer. Only acts while a popup is open; otherwise Escape passes
  // through to whatever else wants it (the engine, an App-local Modal).
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape' || !hasOpenPopup()) return
      e.stopPropagation()
      e.preventDefault()
      closeTopPopup()
    }
    document.addEventListener('keydown', onKeyDown, true)
    return () => document.removeEventListener('keydown', onKeyDown, true)
  }, [])
  return (
    <>
      {snap.map((n, i) => (
        <PopupLayer key={n.id} node={n} isTop={i === snap.length - 1} />
      ))}
    </>
  )
}

type DialogButtonVariant = 'primary' | 'secondary' | 'ghost'

export interface DialogAction {
  /** Returned by the dialog's promise when this button is chosen. */
  id: string
  label: string
  variant?: DialogButtonVariant
}

export interface DialogOptions {
  title: string
  body?: ReactNode
  /** The footer buttons, left→right. One for an alert, two for a confirm, more for a custom dialog. */
  actions: DialogAction[]
  width?: number
  /** Equal-width buttons (see ModalShell). Defaults on for a 2-button dialog. */
  actionsEqual?: boolean
}

/**
 * Open a ModalShell dialog with arbitrary footer actions — confirm, alert (single OK), or a custom
 * set. Resolves the chosen action's `id`, or `null` if dismissed (Escape / scrim / X).
 */
export function showDialog(opts: DialogOptions): Promise<string | null> {
  return new Promise((resolve) => {
    let settled = false
    const settle = (value: string | null): void => {
      if (settled) return
      settled = true
      resolve(value)
    }
    openPopup(
      (close) => (
        <ModalShell
          title={opts.title}
          onClose={close}
          width={opts.width ?? 420}
          actionsEqual={opts.actionsEqual ?? opts.actions.length === 2}
          actions={opts.actions.map((a) => (
            <Button
              key={a.id}
              variant={a.variant ?? 'primary'}
              onClick={() => {
                settle(a.id)
                close()
              }}
            >
              {a.label}
            </Button>
          ))}
        >
          {opts.body}
        </ModalShell>
      ),
      { onClose: () => settle(null) } // default dim scrim from PopupHost
    )
  })
}

export interface ConfirmOptions {
  title: string
  body?: ReactNode
  confirmLabel?: string
  cancelLabel?: string
  /** Variant of the confirm button (e.g. a destructive action). Defaults to primary. */
  confirmVariant?: DialogButtonVariant
}

/** Sugar over showDialog for a two-button confirm. Resolves `true` if confirmed, `false` otherwise. */
export function showConfirm(opts: ConfirmOptions): Promise<boolean> {
  return showDialog({
    title: opts.title,
    body: opts.body,
    actions: [
      { id: 'cancel', label: opts.cancelLabel ?? 'Cancel', variant: 'ghost' },
      { id: 'confirm', label: opts.confirmLabel ?? 'Confirm', variant: opts.confirmVariant ?? 'primary' }
    ]
  }).then((r) => r === 'confirm')
}
