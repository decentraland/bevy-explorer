import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfilePassport, type PassportEditing } from '../features/profile/ProfilePassport'
import { applyProfileEdit, isValidLinkUrl, isValidName } from '../features/profile/profileFields'
import { FIELD_OPTIONS } from '../features/profile/profileFieldOptions'
import type { Profile, ProfileEdit } from '../engine/protocol'
import { resetPopups, PopupHost } from '../design'

const profile: Profile = {
  address: '0xme',
  name: 'Mojito',
  hasClaimedName: true,
  isGuest: false,
  description: 'gm from the plaza',
  links: [{ title: 'x', url: 'https://x.com/mojito' }],
  info: { gender: 'Male', realName: 'mo' }
}

function editing(over: Partial<PassportEditing> = {}): PassportEditing {
  return { ownedNames: ['Mojito', 'MojitoDCL'], saving: false, error: null, save: vi.fn(), dismissError: vi.fn(), ...over }
}

const renderPassport = (edit: PassportEditing, self = true, p: Profile = profile): void => {
  render(
    <>
      <ProfilePassport profile={p} isSelf={self} editing={edit} onClose={vi.fn()} />
      <PopupHost />
    </>
  )
}

const openEditor = async (edit: PassportEditing, p: Profile = profile): Promise<void> => {
  renderPassport(edit, true, p)
  await userEvent.click(screen.getByRole('button', { name: 'EDIT PROFILE' }))
}

describe('passport edit mode', () => {
  it('is offered on your own passport only', () => {
    const { unmount } = render(<ProfilePassport profile={profile} isSelf editing={editing()} onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'EDIT PROFILE' })).toBeInTheDocument()
    unmount()

    // Someone else's passport: same profile, isSelf false.
    render(<ProfilePassport profile={profile} editing={editing()} onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: 'EDIT PROFILE' })).not.toBeInTheDocument()
  })

  it('is not offered when the session has no edit plumbing (still loading)', () => {
    render(<ProfilePassport profile={profile} isSelf onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: 'EDIT PROFILE' })).not.toBeInTheDocument()
  })

  it('opens seeded from the current profile', async () => {
    await openEditor(editing())
    expect(screen.getByLabelText('About me')).toHaveValue('gm from the plaza')
    expect(screen.getByLabelText('Link 1 URL')).toHaveValue('https://x.com/mojito')
    expect(screen.getByLabelText('Real Name')).toHaveValue('mo')
  })

  it('SAVE is disabled until something actually changes, and sends ONLY what changed', async () => {
    const edit = editing()
    await openEditor(edit)
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()

    await userEvent.clear(screen.getByLabelText('About me'))
    await userEvent.type(screen.getByLabelText('About me'), 'gm from the beach')
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))

    // A partial update: the engine merges it, so untouched sections must not be restated.
    expect(edit.save).toHaveBeenCalledWith({ description: 'gm from the beach' } satisfies ProfileEdit)
  })

  it('clears a field by sending the whole info section without it', async () => {
    const edit = editing()
    await openEditor(edit)
    await userEvent.clear(screen.getByLabelText('Real Name'))
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    expect(edit.save).toHaveBeenCalledWith({ info: { gender: 'Male' } })
  })

  it('refuses to save a link that is not a real http(s) URL', async () => {
    const edit = editing()
    await openEditor(edit)
    await userEvent.clear(screen.getByLabelText('Link 1 URL'))
    await userEvent.type(screen.getByLabelText('Link 1 URL'), 'javascript:alert(1)')
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()
    expect(screen.getByLabelText('Link 1 URL')).toHaveAttribute('aria-invalid', 'true')
    expect(edit.save).not.toHaveBeenCalled()
  })

  it('caps links at five', async () => {
    const many = { ...profile, links: Array.from({ length: 4 }, (_, i) => ({ title: `l${i}`, url: `https://e.com/${i}` })) }
    await openEditor(editing(), many)
    await userEvent.click(screen.getByRole('button', { name: '+ ADD LINK' }))
    expect(screen.queryByRole('button', { name: '+ ADD LINK' })).not.toBeInTheDocument()
  })

  it('removes a link', async () => {
    const edit = editing()
    await openEditor(edit)
    await userEvent.click(screen.getByRole('button', { name: 'Remove link 1' }))
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    expect(edit.save).toHaveBeenCalledWith({ links: [] })
  })

  it('offers owned names as claimed, and validates a typed one', async () => {
    const edit = editing()
    await openEditor(edit)
    // Select is our own listbox primitive, not a native <select>: open it, then click an option.
    await userEvent.click(screen.getByRole('button', { name: 'Display name' }))
    expect(screen.getByRole('option', { name: 'MojitoDCL' })).toBeInTheDocument()
    await userEvent.click(screen.getByRole('option', { name: 'Custom name…' }))

    const custom = screen.getByLabelText('Custom display name')
    await userEvent.type(custom, 'bad name!')
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()

    await userEvent.clear(custom)
    await userEvent.type(custom, 'mojito2')
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    expect(edit.save).toHaveBeenCalledWith({ name: 'mojito2' })
  })

  it('stays open on a failed save, keeping the edits and showing why', async () => {
    const edit = editing()
    const { rerender } = render(<ProfilePassport profile={profile} isSelf editing={edit} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'EDIT PROFILE' }))
    await userEvent.type(screen.getByLabelText('About me'), '!')

    rerender(<ProfilePassport profile={profile} isSelf editing={editing({ saving: true })} onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'SAVING…' })).toBeDisabled()

    rerender(<ProfilePassport profile={profile} isSelf editing={editing({ error: 'failed to deploy to server.' })} onClose={vi.fn()} />)
    expect(screen.getByRole('alert')).toHaveTextContent('failed to deploy to server.')
    expect(screen.getByLabelText('About me')).toHaveValue('gm from the plaza!')
  })

  it('closes once a save lands', async () => {
    const { rerender } = render(<ProfilePassport profile={profile} isSelf editing={editing()} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'EDIT PROFILE' }))
    rerender(<ProfilePassport profile={profile} isSelf editing={editing({ saving: true })} onClose={vi.fn()} />)
    rerender(<ProfilePassport profile={profile} isSelf editing={editing()} onClose={vi.fn()} />)
    expect(screen.queryByLabelText('About me')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'EDIT PROFILE' })).toBeInTheDocument()
  })

  it('confirms before discarding unsaved edits', async () => {
    resetPopups()
    await openEditor(editing())
    await userEvent.type(screen.getByLabelText('About me'), '!')
    await userEvent.click(screen.getByRole('button', { name: 'CANCEL' }))
    expect(await screen.findByText('Discard changes?')).toBeInTheDocument()
    resetPopups()
  })

  it('keeps a value the option list does not carry, rather than dropping it', async () => {
    const odd = { ...profile, info: { gender: 'Bigender' } }
    await openEditor(editing(), odd)
    expect(screen.getByRole('button', { name: 'Gender' })).toHaveTextContent('Bigender')
  })
})

describe('profile edit helpers', () => {
  it('rejects non-http links, accepts http(s)', () => {
    expect(isValidLinkUrl('https://decentraland.org')).toBe(true)
    expect(isValidLinkUrl('http://decentraland.org')).toBe(true)
    expect(isValidLinkUrl('javascript:alert(1)')).toBe(false)
    expect(isValidLinkUrl('decentraland.org')).toBe(false)
  })

  it('accepts alphanumeric names up to 15 characters', () => {
    expect(isValidName('robtfm')).toBe(true)
    expect(isValidName('rob tfm')).toBe(false)
    expect(isValidName('')).toBe(false)
    expect(isValidName('a'.repeat(16))).toBe(false)
  })

  it('applies an edit the way a save will land', () => {
    const next = applyProfileEdit(profile, { description: '', info: { gender: 'Male' }, links: [] })
    expect(next.description).toBeUndefined()
    expect(next.links).toBeUndefined()
    expect(next.info).toEqual({ gender: 'Male' })
    // Untouched fields survive.
    expect(next.name).toBe('Mojito')
  })

  it('ships dropdown options with no stray whitespace', () => {
    for (const options of Object.values(FIELD_OPTIONS)) {
      for (const option of options ?? []) expect(option).toBe(option.trim())
    }
  })
})

describe('unsaved edits are announced upward', () => {
  const openWith = async (onClose: () => void, onDirtyChange?: (d: boolean) => void): Promise<void> => {
    render(
      <>
        <ProfilePassport
          profile={profile}
          isSelf
          editing={editing()}
          onClose={onClose}
          onDirtyChange={onDirtyChange}
        />
        <PopupHost />
      </>
    )
    await userEvent.click(screen.getByRole('button', { name: 'EDIT PROFILE' }))
  }

  it('announces unsaved edits, so the popup layer can refuse a stray backdrop click', async () => {
    const onDirtyChange = vi.fn()
    await openWith(vi.fn(), onDirtyChange)
    expect(onDirtyChange).toHaveBeenLastCalledWith(false)

    await userEvent.type(screen.getByLabelText('About me'), '!')
    expect(onDirtyChange).toHaveBeenLastCalledWith(true)

    await userEvent.click(screen.getByRole('button', { name: 'CANCEL' }))
    await userEvent.click(screen.getByRole('button', { name: 'Discard' }))
    expect(onDirtyChange).toHaveBeenLastCalledWith(false)
  })
})

describe('clearing a field', () => {
  it('sends an empty description rather than dropping it — the schema requires the key', async () => {
    const edit = editing()
    await openEditor(edit)
    await userEvent.clear(screen.getByLabelText('About me'))
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    // Not undefined and not absent: the bridge turns this into `description: ''`, because a null
    // would remove a key the profile schema requires and the deploy would be rejected.
    expect(edit.save).toHaveBeenCalledWith(expect.objectContaining({ description: '' }))
  })
})

describe('opening the editor changes nothing by itself', () => {
  it('SAVE stays disabled when the passport swaps its profile object mid-edit', async () => {
    // The passport renders the identity-only profile first and the rich fetch replaces it: a new
    // object with the same values, which must not register as an edit.
    const { rerender } = render(
      <>
        <ProfilePassport profile={profile} isSelf editing={editing()} onClose={vi.fn()} />
        <PopupHost />
      </>
    )
    await userEvent.click(screen.getByRole('button', { name: 'EDIT PROFILE' }))
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()

    rerender(
      <>
        <ProfilePassport
          profile={{ ...profile, badges: [], mutuals: 3 }}
          isSelf
          editing={editing()}
          onClose={vi.fn()}
        />
        <PopupHost />
      </>
    )
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()
  })

  it('does not count the stored formatting as an edit', async () => {
    // Whitespace the profile happens to carry is normalised on BOTH sides of the comparison, so it
    // is not a change until the user makes one.
    await openEditor(editing(), { ...profile, description: '  gm  ', links: [{ title: ' x ', url: ' https://x.com ' }] })
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()
  })
})

describe('edit mode keeps the passport layout still', () => {
  it('keeps a bar where the tabs were, so the avatar does not jump', async () => {
    await openEditor(editing())
    // The tabs stand down (there is nothing to switch to) but their bar remains, labelled.
    expect(screen.queryByRole('button', { name: 'OVERVIEW' })).toBeNull()
    const bar = screen.getByText('EDIT PROFILE')
    expect(bar.tagName).toBe('SPAN') // a label, not the button that opened this
  })
})
