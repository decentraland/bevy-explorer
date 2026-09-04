// "This link carries parameters the Explorer doesn't recognise" — an ordinary dialog on the popup
// layer (nothing is broken, the params were just ignored), listing what was dropped and, so the
// author can fix the link, everything the entry url does accept (lib/entryParams.ts).

import { openPopup, ModalShell, Button } from '../../design'
import { acceptedEntryParams } from '../../lib/entryParams'
import styles from './EntryParamsDialog.module.css'

/** Open the dialog; the returned handle closes it (App's effect cleanup). */
export function openEntryParamsDialog(unrecognised: string[]): () => void {
  return openPopup((close) => (
    <ModalShell
      title="Unrecognised link parameters"
      role="alertdialog"
      ariaLabel="Unrecognised link parameters"
      width={560}
      onClose={close}
      actionsAlign="center"
      actions={
        <Button variant="primary" onClick={close}>
          OK
        </Button>
      }
    >
      <p className={styles.lead}>
        This link carries {unrecognised.length > 1 ? 'parameters' : 'a parameter'} the Explorer does not
        recognise, so {unrecognised.length > 1 ? 'they were' : 'it was'} ignored:
      </p>
      <ul className={styles.ignored}>
        {unrecognised.map((name) => (
          <li key={name}>{name}</li>
        ))}
      </ul>
      <p className={styles.lead}>The parameters a link can carry:</p>
      <dl className={styles.accepted}>
        {acceptedEntryParams().map((p) => (
          <div key={p.name}>
            <dt>
              <span className={styles.name}>{p.name}</span>
            </dt>
            <dd>{p.doc}</dd>
          </div>
        ))}
      </dl>
    </ModalShell>
  ))
}
