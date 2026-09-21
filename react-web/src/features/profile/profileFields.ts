// The about-me fields, in one table: the passport renders them read-only from it, the edit form
// builds its controls from it, and the tests assert against it. Splitting the label list from the
// input list is how the old scene ended up with fields you could see but not edit.

import type { ProfileInfo } from '../../engine/protocol'

/** How a field is edited. Text fields are free-form; select fields come from FIELD_OPTIONS. */
export type FieldKind = 'text' | 'select' | 'date'

export interface ProfileField {
  key: keyof ProfileInfo
  label: string
  kind: FieldKind
}

/** Order is the render order in both the read-only grid and the edit form. */
export const PROFILE_FIELDS: ProfileField[] = [
  { key: 'gender', label: 'Gender', kind: 'select' },
  { key: 'birthdate', label: 'Birth Date', kind: 'date' },
  { key: 'pronouns', label: 'Pronouns', kind: 'select' },
  { key: 'relationship', label: 'Relationship Status', kind: 'select' },
  { key: 'sexualOrientation', label: 'Sexual Orientation', kind: 'select' },
  { key: 'language', label: 'Language', kind: 'select' },
  { key: 'country', label: 'Country', kind: 'select' },
  { key: 'profession', label: 'Profession', kind: 'text' },
  { key: 'employment', label: 'Employment Status', kind: 'select' },
  { key: 'hobby', label: 'Favorite Hobby', kind: 'text' },
  { key: 'realName', label: 'Real Name', kind: 'text' }
]

// Limits, matched to unity-explorer so the same profile is editable in both clients: the
// description field's characterLimit (400), the add-link modal's title limit (15, url unlimited)
// and LINKS_MAX_AMOUNT (5). A name is 15 — the length the DCL account site allows.
export const DESCRIPTION_MAX = 400
export const LINK_TITLE_MAX = 15
export const MAX_LINKS = 5
export const NAME_MAX = 15

/** Unclaimed names are the alphanumeric subset — anything else can't be deployed. (A CLAIMED name
 *  is picked from the owned list, so it isn't checked against this.) */
export const NAME_PATTERN = /^[a-zA-Z0-9]{1,15}$/

export const isValidName = (name: string): boolean => NAME_PATTERN.test(name)

/** Links must be openable. The passport renders them as real anchors, so anything that isn't
 *  http(s) is either broken or a `javascript:` URL we should never render. */
export function isValidLinkUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return parsed.protocol === 'http:' || parsed.protocol === 'https:'
  } catch {
    return false
  }
}
