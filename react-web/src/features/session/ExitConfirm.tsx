// Confirm dialog shown when the back gesture / Back button would leave the world (see useExitGuard).
import { ModalShell, Button, openPopup } from '../../design'

export function ExitConfirm({ onStay, onLeave }: { onStay: () => void; onLeave: () => void }): React.JSX.Element {
  return (
    <ModalShell
      title="Leave Decentraland?"
      closeButton={false}
      width={420}
      actions={
        <>
          <Button variant="ghost" onClick={onStay}>
            Stay
          </Button>
          <Button variant="primary" onClick={onLeave}>
            Leave
          </Button>
        </>
      }
      actionsEqual
    >
      You&apos;re about to exit the world and go back. You&apos;ll have to reconnect to return.
    </ModalShell>
  )
}

/** Open the leave-confirmation as a popup; returns the close handle. Living in the popup layer means
 *  hasOpenPopup() covers it (Enter can't focus the chat behind it) and Escape / scrim-click resolve to
 *  "stay" via the onClose contract — any close path other than Leave keeps the user in-world. */
export function openExitConfirm(onStay: () => void, onLeave: () => void): () => void {
  return openPopup(
    (close) => (
      <ExitConfirm
        onStay={close}
        onLeave={() => {
          close()
          onLeave()
        }}
      />
    ),
    { onClose: onStay }
  )
}
