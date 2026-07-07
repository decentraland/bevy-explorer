import { Fragment, useSyncExternalStore, type ReactNode } from 'react'
import { ModalShell } from './Modal'
import { Button } from './Button'

/** A popup is a render function given its own `close` callback; it returns the overlay to render. */
type PopupRender = (close: () => void) => ReactNode

// Module-level popup stack — a single HUD-wide layer (like the hoverPos store), NOT React state.
// Plain functions mutate it and notify subscribers, so a popup can be opened from anywhere (a
// component, an event handler, a util) without prop-threading or a hook — and a popup can open
// another (community → passport → confirm). <PopupHost/>, mounted once, subscribes and renders it.
let stack: { id: number; render: PopupRender }[] = []
let nextId = 0
const listeners = new Set<() => void>()
const emit = (): void => listeners.forEach((l) => l())
const closeById = (id: number): void => {
  stack = stack.filter((n) => n.id !== id)
  emit()
}

/** Open an arbitrary popup imperatively; returns a `close` handle. Callable from anywhere. */
export function openPopup(render: PopupRender): () => void {
  const id = ++nextId
  stack = [...stack, { id, render }]
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

/** Mounted once (see main.tsx) — the single React subscriber that renders the popup stack. */
export function PopupHost(): React.JSX.Element {
  const snap = useSyncExternalStore(subscribe, getSnapshot)
  return (
    <>
      {snap.map((n) => (
        <Fragment key={n.id}>{n.render(() => closeById(n.id))}</Fragment>
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
    const done = (value: string | null, close: () => void): void => {
      if (!settled) {
        settled = true
        resolve(value)
      }
      close()
    }
    openPopup((close) => (
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
    ))
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
