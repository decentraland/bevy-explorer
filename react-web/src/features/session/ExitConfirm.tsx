// Confirm dialog shown when the back gesture / Back button would leave the world (see useExitGuard).
import { ModalShell, Button } from '../../design'

export function ExitConfirm({ onStay, onLeave }: { onStay: () => void; onLeave: () => void }): React.JSX.Element {
  return (
    <ModalShell
      title="Leave Decentraland?"
      onClose={onStay}
      closeButton={false}
      dismissOnScrim={false}
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
