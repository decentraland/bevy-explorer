// "Can't travel there" — an in-world realm change the destination check refused. The player stays
// where they are (Unity's RealmNavigator shows the same kind of notice instead of leaving).

import { Button, ModalShell, openPopup } from '../../design'

export function openTravelError(message: string, onClose: () => void): () => void {
  return openPopup(
    (close) => (
      <ModalShell title="Can't travel there" width={400} closeButton={false} ariaLabel="Can't travel there" actions={<Button onClick={close}>OK</Button>}>
        <p>{message}</p>
      </ModalShell>
    ),
    { onClose }
  )
}
