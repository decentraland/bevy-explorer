// What a profile edit looks like once it lands — shared by the session (optimistic update while
// the deploy is in flight) and the mock bridge (its stand-in for the catalyst). Lives beside the
// protocol rather than in features/: it describes the wire's semantics, not a screen.

import type { Profile, ProfileEdit, ProfileInfo } from './protocol'

/** A name that isn't one of the wallet's claimed NAMEs is shown with four hex digits of the
 *  address appended — the engine builds nametags the same way (`crates/avatar`) and the bridge
 *  answers with the same, so the passport must not flash a bare name in between. */
export function applyProfileEdit(profile: Profile, edit: ProfileEdit, ownedNames: readonly string[]): Profile {
  const next = { ...profile }
  if (edit.name !== undefined) {
    const name = edit.name
    const claimed = ownedNames.some((n) => n.toLowerCase() === name.toLowerCase())
    next.name = claimed ? name : `${name}#${profile.address.slice(-4)}`
    next.hasClaimedName = claimed
  }
  if (edit.description !== undefined) next.description = edit.description.trim() || undefined
  if (edit.links !== undefined) next.links = edit.links.length > 0 ? edit.links : undefined
  if (edit.info !== undefined) {
    const info: ProfileInfo = {}
    for (const [key, value] of Object.entries(edit.info) as [keyof ProfileInfo, string | undefined][]) {
      if (value != null && value !== '') info[key] = value
    }
    next.info = Object.keys(info).length > 0 ? info : undefined
  }
  return next
}
