import { Fragment, useSyncExternalStore, type ReactNode } from 'react'
import { ModalShell } from './Modal'
import { Button } from './Button'
import styles from './popups.module.css'

/** A popup is a render function given its own `close` callback; it returns the overlay to render. */
type PopupRender = (close: () => void) => ReactNode

/** Per-popup options. By default the popup layer draws a full-screen backdrop that closes the popup
 *  when clicked; set `backdrop: false` for popups that draw their own scrim (dialogs / the passport). */
export interface PopupOptions {
  backdrop?: boolean
  backdropClickCloses?: boolean
}
const DEFAULTS: Required<PopupOptions> = { backdrop: true, backdropClickCloses: true }

// Module-level popup stack — a single HUD-wide layer (like the hoverPos store), NOT React state.
// Plain functions mutate it and notify subscribers, so a popup can be opened from anywhere (a
// component, an event handler, a util) without prop-threading or a hook — and a popup can open
// another (community → passport → confirm). <PopupHost/>, mounted once, subscribes and renders it.
let stack: { id: number; render: PopupRender; options: Required<PopupOptions> }[] = []
let nextId = 0
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())
const closeById = (id: number): void => {
  stack = stack.filter((n) => n.id !== id)
  emit()
}

/** Close the topmost popup (no-op if the stack is empty). Driven by the engine's 'Cancel' system
 *  action (Escape) relayed through the bridge — see useEngineSession — so it works even while the
 *  engine holds keyboard focus. Closes one layer at a time, so stacked popups dismiss in order. */
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

/** Clear the popup stack — for tests (the store is a module singleton, so it leaks across tests). */
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

/** Mounted once at the HUD root (see App) — the single React subscriber that renders the popup stack.
 *  It has no transformed ancestor, so a popup's own `position: fixed` resolves against the viewport;
 *  no portal is needed (the passport / dialogs already rely on that for their inline scrims). */
export function PopupHost(): React.JSX.Element {
  const snap = useSyncExternalStore(subscribe, getSnapshot)
  return (
    <>
      {snap.map((n) => {
        const close = (): void => closeById(n.id)
        const content = n.render(close)
        // No backdrop → the content owns its own scrim (dialogs, passport). Otherwise draw a
        // full-screen fixed backdrop behind it; `.backdrop` has no transform, so it neither shifts
        // nor clips the popup's own fixed positioning.
        if (!n.options.backdrop) return <Fragment key={n.id}>{content}</Fragment>
        return (
          <div key={n.id} className={styles.backdrop} onClick={n.options.backdropClickCloses ? close : undefined}>
            {content}
          </div>
        )
      })}
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
    const done = (value: string | null, close: () => void): void => {
      if (!settled) {
        settled = true
        resolve(value)
      }
      close()
    }
    openPopup(
      (close) => (
        <ModalShell
          title={opts.title}
          onClose={() => done(null, close)}
          width={opts.width ?? 420}
          actionsEqual={opts.actionsEqual ?? opts.actions.length === 2}
          actions={opts.actions.map((a) => (
            <Button key={a.id} variant={a.variant ?? 'primary'} onClick={() => done(a.id, close)}>
              {a.label}
            </Button>
          ))}
        >
          {opts.body}
        </ModalShell>
      ),
      { backdrop: false } // ModalShell draws its own scrim
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
