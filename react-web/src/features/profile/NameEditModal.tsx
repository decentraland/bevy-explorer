// Editing your NAME, in its own popup — reached by the pencil beside the name on your passport.
//
// It is separate from the rest of profile editing because the choice is structural rather than a
// field: a wallet holding claimed NAMEs picks one from a list, and anyone can instead set a
// free-text name, which the protocol makes unique by appending four hex digits of the address.
// (Mirrors unity-explorer's Edit Username dialog.)

import { useEffect, useRef, useState } from 'react'
import { Button, ModalShell, Select, Tabs, TextInput, openPopup, type TabItem } from '../../design'
import { useSession } from '../session/SessionContext'
import { splitName } from '../../lib/identity'
import { NAME_MAX, isValidName } from './profileFields'
import styles from './NameEditModal.module.css'

/** Where a NAME is bought. Same destination as the communities modal's "Get a NAME". */
const NAMES_URL = 'https://decentraland.org/marketplace/names/claim'

/** What the protocol appends to a non-claimed name: the last four characters of the address (the
 *  engine builds nametags the same way — see `crates/avatar/src/lib.rs`). Derived from the address
 *  rather than read off the current name, which carries no suffix at all while a NAME is claimed. */
const addressSuffix = (address: string): string => (address === '' ? '' : `#${address.slice(-4)}`)

type Tab = 'unique' | 'custom'
const NAME_TABS: TabItem<Tab>[] = [
  { id: 'unique', label: 'UNIQUE NAME' },
  { id: 'custom', label: 'NON-UNIQUE USERNAME' }
]

export function NameEditModal({ onClose }: { onClose: () => void }): React.JSX.Element {
  const session = useSession()
  const { ownedNames, saving, saveError, save, dismissSaveError } = session.profile
  const profile = session.profile.data
  const currentName = profile?.name ?? ''
  const { base } = splitName(currentName)
  const suffix = addressSuffix(profile?.address ?? '')
  const claimed = profile?.hasClaimedName === true

  const hasNames = ownedNames.length > 0
  const [tab, setTab] = useState<Tab>(() => (claimed && hasNames ? 'unique' : 'custom'))
  const [picked, setPicked] = useState(() => (claimed ? currentName : (ownedNames[0] ?? '')))
  // The passport asks for the list when it opens, but that is a catalyst round trip and this popup
  // can open first. When the list lands, a claimed NAME's owner belongs on the picker — once; a
  // tab they chose themselves afterwards is theirs to keep.
  const hadNames = useRef(hasNames)
  useEffect(() => {
    if (hadNames.current || !hasNames) return
    hadNames.current = true
    if (claimed) setTab('unique')
    setPicked((p) => (p === '' ? ownedNames[0] : p))
  }, [hasNames, claimed, ownedNames])
  // Only the part before the '#': the suffix is the protocol's, not something you type.
  const [typed, setTyped] = useState(base)

  // Close once a save has actually landed — a rejected deploy comes back as an error to show here.
  const wasSaving = useRef(false)
  useEffect(() => {
    if (wasSaving.current && !saving && saveError == null) onClose()
    wasSaving.current = saving
  }, [saving, saveError, onClose])

  // A claimed NAME can only be picked once the list has arrived; a typed one must be deployable.
  // The typed field holds only the part before the '#', so it is compared against that part of the
  // current name — comparing it against the whole thing would make an untouched name look edited.
  const chosen = tab === 'unique' ? picked : typed.trim()
  const valid = tab === 'unique' ? chosen !== '' : isValidName(chosen)
  const unchanged = chosen === (tab === 'unique' ? currentName : base)
  const canSave = valid && !unchanged && !saving

  return (
    <ModalShell
      title="Edit Username"
      onClose={onClose}
      width={520}
      bodyClassName={styles.body}
    >
      {hasNames && (
        <Tabs
          variant="underline"
          items={NAME_TABS}
          value={tab}
          onChange={(id) => {
            dismissSaveError()
            setTab(id)
          }}
          aria-label="Name type"
        />
      )}

      {tab === 'unique' ? (
        <Select
          aria-label="Claimed name"
          variant="light"
          value={picked}
          options={ownedNames.map((n) => ({ value: n, label: n }))}
          onChange={setPicked}
        />
      ) : (
        <>
          <div className={styles.field}>
            <TextInput
              aria-label="Username"
              variant="light"
              value={typed}
              maxLength={NAME_MAX}
              invalid={typed.trim() !== '' && !isValidName(typed.trim())}
              onChange={(v) => {
                dismissSaveError()
                setTyped(v)
              }}
            />
            {/* The protocol appends this to any non-claimed name; showing it inside the field is
                how unity-explorer explains where it comes from. */}
            <span className={styles.suffix}>{suffix}</span>
          </div>
          <div className={styles.count}>
            {typed.trim().length}/{NAME_MAX}
          </div>
          {typed.trim() !== '' && !isValidName(typed.trim()) && (
            <div className={styles.hint}>Letters and numbers only, up to {NAME_MAX} characters.</div>
          )}
        </>
      )}

      {saveError != null && <div className={styles.error}>{saveError}</div>}

      {/* The actions live in the body rather than the shell's footer: the claim panel sits BELOW
          them, filling the bottom of the dialog, and a footer would land under it. */}
      <div className={styles.actions}>
        <Button variant="secondary" onClick={onClose} disabled={saving}>
          CANCEL
        </Button>
        <Button variant="primary" onClick={() => save({ name: chosen })} disabled={!canSave}>
          {saving ? 'SAVING…' : 'SAVE'}
        </Button>
      </div>

      <div className={styles.promo}>
        <h3 className={styles.promoTitle}>
          {hasNames ? 'Unlock more possibilities with multiple NAMEs!' : 'Claim a unique NAME for the full Decentraland experience!'}
        </h3>
        <p className={styles.promoBody}>
          {hasNames
            ? 'Each NAME adds a World to your collection and increases your overall World size limits. Plus, NAMEs are each worth 100 Voting Power!'
            : 'A NAME comes with a World you can build and host events in, and grants DAO Voting Power too!'}
        </p>
        <Button variant="primary" onClick={() => window.open(NAMES_URL, '_blank', 'noopener')}>
          CLAIM NAME
        </Button>
      </div>
    </ModalShell>
  )
}

/** Open the name editor over the passport. */
export function openNameEdit(): () => void {
  return openPopup((close) => <NameEditModal onClose={close} />)
}
