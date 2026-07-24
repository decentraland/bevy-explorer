// Modal family — the dialog card, plus the ModalShell layout wrapper, ModalTitle header and
// ModalActions footer. Rebuilt in TS + CSS Modules; design references eordano/dcl-react-ui Modal.
//
// The card is presentational only: the overlay (scrim, DPI scale, entrance animation, focus trap,
// Escape) is owned by whoever hosts it — PopupHost for popup-layer dialogs, or a self-contained
// surface like EngineErrorModal. So Modal draws no backdrop and does no portalling.

import { Children, Fragment, isValidElement, type ReactNode } from 'react'
import styles from './Modal.module.css'

interface ModalProps {
  children: ReactNode
  width?: number
  ariaLabel?: string
  role?: string
  className?: string
}

/** The dialog card — presentational only. Whoever hosts it owns the scrim, DPI scale, entrance
 *  animation, focus trap and Escape (PopupHost, or a self-contained surface). stopPropagation keeps a
 *  click on the card from reaching the host's scrim (and closing it). */
export function Modal({
  children,
  width = 420,
  ariaLabel,
  role = 'dialog',
  className = ''
}: ModalProps): React.JSX.Element {
  return (
    <div
      className={`${styles.card} ${className}`.trim()}
      style={{ width }}
      role={role}
      aria-modal="true"
      aria-label={ariaLabel}
      onClick={(e) => e.stopPropagation()}
    >
      {children}
    </div>
  )
}

interface ModalTitleProps {
  title?: ReactNode
  subtitle?: ReactNode
  icon?: ReactNode
  onClose?: () => void
  onBack?: () => void
  closeLabel?: string
  backLabel?: string
  closeSize?: number
  centered?: boolean
  className?: string
}

export function ModalTitle({
  title,
  subtitle,
  icon,
  onClose,
  onBack,
  closeLabel = 'Close',
  backLabel = 'Back',
  closeSize = 14,
  centered = false,
  className = ''
}: ModalTitleProps): React.JSX.Element {
  return (
    <header
      className={`${styles.title} ${centered ? styles.titleCentered : ''} ${className}`.trim()}
    >
      {onBack ? (
        <button
          type="button"
          className={styles.back}
          aria-label={backLabel}
          onClick={onBack}
        >
          <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
            <path
              d="M15 5l-7 7 7 7"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              fill="none"
            />
          </svg>
        </button>
      ) : null}
      <div className={styles.titleText}>
        <div className={styles.titleHeading}>
          {icon ? (
            <span className={styles.icon} aria-hidden="true">
              {icon}
            </span>
          ) : null}
          {title}
        </div>
        {subtitle ? <div className={styles.subtitle}>{subtitle}</div> : null}
      </div>
      {onClose ? (
        <button
          type="button"
          className={styles.close}
          aria-label={closeLabel}
          onClick={onClose}
        >
          <svg
            viewBox="0 0 24 24"
            width={closeSize}
            height={closeSize}
            aria-hidden="true"
          >
            <path
              d="M6 6l12 12M18 6L6 18"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
            />
          </svg>
        </button>
      ) : null}
    </header>
  )
}

type ActionsDirection = 'row' | 'column'
type ActionsAlign = 'stretch' | 'start' | 'center' | 'end' | 'between'

interface ModalActionsProps {
  children: ReactNode
  lead?: ReactNode
  equal?: boolean
  direction?: ActionsDirection
  align?: ActionsAlign
  className?: string
}

// Flatten children, unwrapping fragments, so `equal` mode produces one column
// per button even when actions are passed as <>…</> (which React reports as a
// single child). Without this a fragment collapses into one column.
function flattenChildren(children: ReactNode): ReactNode[] {
  const out: ReactNode[] = []
  Children.forEach(children, (child) => {
    if (child == null || child === false) return
    if (isValidElement(child) && child.type === Fragment) {
      const props = child.props as { children?: ReactNode }
      out.push(...flattenChildren(props.children))
    } else {
      out.push(child)
    }
  })
  return out
}

export function ModalActions({
  children,
  lead,
  equal = false,
  direction = 'row',
  align = 'stretch',
  className = ''
}: ModalActionsProps): React.JSX.Element {
  const isColumn = direction === 'column'
  const justify =
    align === 'start'
      ? 'flex-start'
      : align === 'center'
        ? 'center'
        : align === 'end'
          ? 'flex-end'
          : align === 'between'
            ? 'space-between'
            : undefined

  const wrapped = equal
    ? flattenChildren(children).map((child, i) => (
        <div className={styles.actionBtn} key={i}>
          {child}
        </div>
      ))
    : children

  // With a lead note the buttons must stay grouped so they wrap together (and
  // stay right-aligned) instead of the note being crushed into a thin column.
  const hasLead = lead != null && !isColumn
  const actions = hasLead ? <div className={styles.group}>{wrapped}</div> : wrapped

  return (
    <div
      className={`${styles.actions} ${isColumn ? styles.actionsColumn : ''} ${
        hasLead ? styles.actionsLead : ''
      } ${className}`.trim()}
      style={justify ? { justifyContent: justify } : undefined}
    >
      {lead != null ? <div className={styles.lead}>{lead}</div> : null}
      {actions}
    </div>
  )
}

const SIZES: Record<string, number> = {
  sm: 420,
  tiny: 420,
  md: 540,
  small: 540,
  lg: 720,
  large: 720,
  xl: 900
}

interface ModalShellProps {
  children: ReactNode
  /** Runs from the header close (X) button, when `closeButton` is on. */
  onClose?: () => void
  width?: number
  size?: keyof typeof SIZES
  className?: string
  ariaLabel?: string
  role?: string
  bodyless?: boolean
  bodyClassName?: string
  header?: ReactNode
  title?: ReactNode
  subtitle?: ReactNode
  icon?: ReactNode
  onBack?: () => void
  closeButton?: boolean
  centeredTitle?: boolean
  closeSize?: number
  actions?: ReactNode
  actionsEqual?: boolean
  actionsDirection?: ActionsDirection
  actionsAlign?: ActionsAlign
  actionsLead?: ReactNode
  actionsClassName?: string
}

export function ModalShell({
  children,
  onClose,
  width,
  size = 'sm',
  className = '',
  ariaLabel,
  role = 'dialog',
  bodyless = false,
  bodyClassName = '',
  header,
  title,
  subtitle,
  icon,
  onBack,
  closeButton = true,
  centeredTitle = false,
  closeSize,
  actions,
  actionsEqual = false,
  actionsDirection = 'row',
  actionsAlign = 'stretch',
  actionsLead,
  actionsClassName
}: ModalShellProps): React.JSX.Element {
  const resolvedWidth = width != null ? width : (SIZES[size] ?? SIZES.sm)

  const hasAutoHeader =
    title != null || subtitle != null || icon != null || onBack != null
  const headerNode =
    header != null ? (
      header
    ) : hasAutoHeader ? (
      <ModalTitle
        title={title}
        subtitle={subtitle}
        icon={icon}
        onBack={onBack}
        onClose={closeButton ? onClose : undefined}
        centered={centeredTitle}
        closeSize={closeSize}
      />
    ) : null

  const footerNode =
    actions != null ? (
      <ModalActions
        equal={actionsEqual}
        direction={actionsDirection}
        align={actionsAlign}
        lead={actionsLead}
        className={actionsClassName}
      >
        {actions}
      </ModalActions>
    ) : null

  const body = bodyless ? (
    children
  ) : (
    <div className={`${styles.shellBody} ${bodyClassName}`.trim()}>{children}</div>
  )

  return (
    <Modal width={resolvedWidth} className={`${styles.shell} ${className}`.trim()} ariaLabel={ariaLabel} role={role}>
      {headerNode}
      {body}
      {footerNode}
    </Modal>
  )
}
