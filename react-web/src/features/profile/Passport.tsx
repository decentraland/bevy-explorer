// Smart wrapper for the full-screen passport: resolves a user by address from the profile store, kicks off
// the rich-profile fetch, and renders the presentational ProfilePassport. Opened as a popup via
// openPassport() (from the profile card's "View Passport" and the sidebar's own profile), so it lives
// in the HUD-wide popup layer and reads the session via useSession() like the profile card.
import { useEffect } from 'react'
import { openPopup, showConfirm } from '../../design'
import { useSession } from '../session/SessionContext'
import { requestPassport, useProfile } from '../session/profileStore'
import { relationshipOf } from '../../lib/relationship'
import { openNameEdit } from './NameEditModal'
import { ProfilePassport } from './ProfilePassport'
import type { Profile } from '../../engine/protocol'

export function Passport({
  userId,
  onClose,
  onDirtyChange
}: {
  userId: string
  onClose: () => void
  onDirtyChange?: (dirty: boolean) => void
}): React.JSX.Element {
  const session = useSession()
  const { requestOwnedNames } = session.profile
  // Fetch the rich profile (badges/photos/about) on open; render identity-only until it lands.
  useEffect(() => {
    requestPassport(userId)
  }, [userId])

  const a = userId.toLowerCase()
  const isSelf = !!session.profile.data && session.profile.data.address.toLowerCase() === a
  const known = useProfile(userId)
  const profile: Profile =
    known ??
    (isSelf && session.profile.data
      ? session.profile.data
      : { address: userId, name: userId, hasClaimedName: false, isGuest: false })

  // Only your own passport can be edited, so the claimed-name list is only worth fetching there.
  useEffect(() => {
    if (isSelf) requestOwnedNames()
  }, [isSelf, requestOwnedNames])

  return (
    <ProfilePassport
      profile={profile}
      isSelf={isSelf}
      onDirtyChange={onDirtyChange}
      editing={
        isSelf && session.profile.data != null
          ? {
              saving: session.profile.saving,
              error: session.profile.saveError,
              save: session.profile.save,
              dismissError: session.profile.dismissSaveError,
              editName: () => void openNameEdit()
            }
          : undefined
      }
      relationship={relationshipOf(session.friends, userId)}
      onAddFriend={(address) => session.friends.act('request', address)}
      onClose={onClose}
    />
  )
}

/** Open a user's full-screen passport as a popup. */
export function openPassport(userId: string): () => void {
  // Unsaved edits, kept current by the passport. A stray click on the scrim is refused outright;
  // a deliberate close — the ×, or the Escape/Cancel action, which the popup layer routes through
  // the same guard — asks first.
  const dirty = { current: false }
  return openPopup(
    (close) => (
      <Passport
        userId={userId}
        onClose={close}
        onDirtyChange={(d) => {
          dirty.current = d
        }}
      />
    ),
    {
      backdropClickCloses: () => !dirty.current,
      // Sync `true` when there is nothing to lose, so an ordinary close stays as immediate as it
      // was; only an edit in progress opens the dialog.
      confirmClose: () =>
        !dirty.current ||
        showConfirm({
          title: 'Discard changes?',
          body: 'Your edits to this profile will be lost.',
          confirmLabel: 'Discard',
          cancelLabel: 'Keep editing'
        })
    }
  )
}
