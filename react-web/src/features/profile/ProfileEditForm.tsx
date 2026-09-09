// Own-profile edit mode: the form that replaces the passport's About card when you press EDIT on
// your own passport. Drafts locally and saves once — the engine deploys a new profile version per
// save, so a field-by-field save would deploy eleven times for one visit.

import { useEffect, useMemo, useRef, useState } from 'react'
import { Button, FieldLabel, Select, TextArea, TextInput, Trash, showConfirm } from '../../design'
import type { Profile, ProfileEdit, ProfileInfo } from '../../engine/protocol'
import { FIELD_OPTIONS } from './profileFieldOptions'
import {
  DESCRIPTION_MAX,
  LINK_TITLE_MAX,
  MAX_LINKS,
  PROFILE_FIELDS,
  isValidLinkUrl
} from './profileFields'
import styles from './ProfileEditForm.module.css'

type LinkDraft = { title: string; url: string }

interface Draft {
  description: string
  info: ProfileInfo
  links: LinkDraft[]
}

const draftOf = (profile: Profile): Draft => ({
  description: profile.description ?? '',
  info: { ...(profile.info ?? {}) },
  links: (profile.links ?? []).map((l) => ({ ...l }))
})

const trimmedInfo = (info: ProfileInfo): ProfileInfo => {
  const out: ProfileInfo = {}
  for (const [key, value] of (Object.entries(info) as [keyof ProfileInfo, string | undefined][]).sort(([a], [b]) =>
    a.localeCompare(b)
  )) {
    if (value != null && value.trim() !== '') out[key] = value.trim()
  }
  return out
}

/** Links as they would be saved: empties dropped, values trimmed, keys in a fixed order — so the
 *  comparison below can't see a difference that is really just the shape the profile came in. */
const trimmedLinks = (links: LinkDraft[]): LinkDraft[] =>
  links.filter((l) => l.url.trim() !== '').map((l) => ({ title: (l.title ?? '').trim(), url: l.url.trim() }))

/** Only what actually changed goes on the wire: the engine merges a partial update, so an
 *  untouched section is better left out than restated.
 *
 *  Both sides go through the same normalization: comparing a trimmed draft against the profile's
 *  raw value made stored whitespace (or a link whose keys arrived in another order) look like an
 *  edit nobody made. */
function editOf(base: Draft, draft: Draft): ProfileEdit {
  const edit: ProfileEdit = {}
  if (draft.description.trim() !== base.description.trim()) edit.description = draft.description.trim()
  const links = trimmedLinks(draft.links)
  if (JSON.stringify(links) !== JSON.stringify(trimmedLinks(base.links))) edit.links = links
  const info = trimmedInfo(draft.info)
  if (JSON.stringify(info) !== JSON.stringify(trimmedInfo(base.info))) edit.info = info
  return edit
}

export function ProfileEditForm({
  profile,
  saving,
  error,
  onSave,
  onCancel,
  onDismissError,
  onStatusChange,
  saveRef
}: {
  profile: Profile
  saving: boolean
  error: string | null
  onSave: (edit: ProfileEdit) => void
  onCancel: () => void
  onDismissError: () => void
  /** Report editability upward: SAVE lives in the passport header (always in view — the form is
   *  taller than the panel), and the passport guards its close paths on `dirty`. */
  onStatusChange?: (status: { dirty: boolean; canSave: boolean }) => void
  /** Filled with the save trigger, so the header's button can fire this form's save. */
  saveRef?: React.MutableRefObject<(() => void) | null>
}): React.JSX.Element {
  const [draft, setDraft] = useState<Draft>(() => draftOf(profile))
  const edit = useMemo(() => editOf(draftOf(profile), draft), [profile, draft])
  const dirty = Object.keys(edit).length > 0

  const badLinks = draft.links.some((l) => l.url.trim() !== '' && !isValidLinkUrl(l.url.trim()))
  const canSave = dirty && !badLinks && !saving

  // Primitive deps only: `edit` is a fresh object every keystroke, so depending on it here would
  // re-notify the parent on every render.
  useEffect(() => {
    onStatusChange?.({ dirty, canSave })
  }, [dirty, canSave, onStatusChange])
  const latestSave = useRef<() => void>(() => {})
  latestSave.current = () => onSave(edit)
  useEffect(() => {
    if (saveRef == null) return
    saveRef.current = () => latestSave.current()
    return () => {
      saveRef.current = null
    }
  }, [saveRef])

  const set = <K extends keyof Draft>(key: K, value: Draft[K]): void => setDraft((d) => ({ ...d, [key]: value }))
  const setInfo = (key: keyof ProfileInfo, value: string): void =>
    setDraft((d) => ({ ...d, info: { ...d.info, [key]: value } }))
  const setLink = (index: number, patch: Partial<LinkDraft>): void =>
    setDraft((d) => ({ ...d, links: d.links.map((l, i) => (i === index ? { ...l, ...patch } : l)) }))

  const cancel = async (): Promise<void> => {
    if (
      dirty &&
      !(await showConfirm({
        title: 'Discard changes?',
        body: 'Your edits to this profile will be lost.',
        confirmLabel: 'Discard',
        cancelLabel: 'Keep editing'
      }))
    ) {
      return
    }
    onCancel()
  }

  return (
    <section className={styles.form} aria-label="Edit profile">
      {error != null && (
        <div className={styles.error} role="alert">
          <span>{error}</span>
          <button type="button" className={styles.errorDismiss} aria-label="Dismiss error" onClick={onDismissError}>
            ×
          </button>
        </div>
      )}

      <h2 className={styles.title}>About Me</h2>
      <TextArea
        aria-label="About me"
        value={draft.description}
        onChange={(value) => set('description', value)}
        maxLength={DESCRIPTION_MAX}
        counter
        rows={4}
        disabled={saving}
        placeholder="Tell people about yourself"
      />

      <h2 className={styles.title}>Info</h2>
      <div className={styles.fields}>
        {PROFILE_FIELDS.map(({ key, label, kind }) => (
          <div key={key} className={styles.field}>
            <FieldLabel>{label}</FieldLabel>
            {kind === 'select' ? (
              <Select
                aria-label={label}
                value={draft.info[key] ?? ''}
                // The blank option is how a field gets cleared; a value the profile already holds
                // but the list doesn't (an older client's, or one edited elsewhere) is added so
                // opening the editor can't silently drop it.
                options={[
                  { value: '', label: '—' },
                  ...optionsFor(key, draft.info[key]).map((o) => ({ value: o, label: o }))
                ]}
                onChange={(value) => setInfo(key, value)}
                disabled={saving}
              />
            ) : (
              <TextInput
                aria-label={label}
                type={kind === 'date' ? 'date' : 'text'}
                value={draft.info[key] ?? ''}
                onChange={(value) => setInfo(key, value)}
                disabled={saving}
              />
            )}
          </div>
        ))}
      </div>

      <h2 className={styles.title}>Links</h2>
      <p className={styles.hint}>Add up to {MAX_LINKS} links to your website or social networks.</p>
      <div className={styles.links}>
        {draft.links.map((link, i) => {
          const invalid = link.url.trim() !== '' && !isValidLinkUrl(link.url.trim())
          return (
            <div key={i} className={styles.linkRow}>
              <TextInput
                aria-label={`Link ${i + 1} title`}
                value={link.title}
                onChange={(title) => setLink(i, { title })}
                maxLength={LINK_TITLE_MAX}
                placeholder="Title"
                disabled={saving}
              />
              <TextInput
                aria-label={`Link ${i + 1} URL`}
                value={link.url}
                onChange={(url) => setLink(i, { url })}
                invalid={invalid}
                placeholder="https://…"
                disabled={saving}
              />
              <button
                type="button"
                className={styles.removeLink}
                aria-label={`Remove link ${i + 1}`}
                disabled={saving}
                onClick={() => set('links', draft.links.filter((_, at) => at !== i))}
              >
                <Trash size={15} />
              </button>
            </div>
          )
        })}
      </div>
      {draft.links.length < MAX_LINKS && (
        <Button
          variant="secondary"
          size="sm"
          disabled={saving}
          onClick={() => set('links', [...draft.links, { title: '', url: '' }])}
        >
          + ADD LINK
        </Button>
      )}

      <div className={styles.actions}>
        <Button variant="ghost" onClick={() => void cancel()} disabled={saving}>
          CANCEL
        </Button>
      </div>
    </section>
  )
}

/** The list for a field, plus whatever the profile already holds if that isn't in it. */
function optionsFor(key: keyof ProfileInfo, current: string | undefined): string[] {
  const options = FIELD_OPTIONS[key] ?? []
  return current != null && current !== '' && !options.includes(current) ? [current, ...options] : options
}
