import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { NameEditModal, openNameEdit } from '../features/profile/NameEditModal'
import { SessionProvider } from '../features/session/SessionContext'
import { fakeProfileState, fakeSession } from './harness'
import { PopupHost, resetPopups } from '../design'
import type { EngineSession } from '../features/session/useEngineSession'
import type { Profile } from '../engine/protocol'

// COMPONENT: the name editor. A NAME is a different kind of choice from the rest of the profile —
// a claimed one is picked from what the wallet owns, anything else is free text that the protocol
// makes unique with four hex digits of the address — so it lives in its own popup, like
// unity-explorer's Edit Username.
afterEach(resetPopups)

const CLAIMED: Profile = { address: '0x6f395cb5ff248b7b5e877b959ab1078dd0f821cf', name: 'Mojito', hasClaimedName: true, isGuest: false }
const UNCLAIMED: Profile = { address: '0xbeef7cd7', name: 'bevyboy#7cd7', hasClaimedName: false, isGuest: false }

function renderModal(profile: Profile, names: string[], over: Partial<ReturnType<typeof fakeProfileState>> = {}): EngineSession {
  const s = fakeSession()
  s.profile = fakeProfileState({ data: profile, ownedNames: names, ...over })
  render(
    <SessionProvider value={s}>
      <NameEditModal onClose={vi.fn()} />
      <PopupHost />
    </SessionProvider>
  )
  return s
}

describe('with claimed NAMEs', () => {
  it('offers both kinds of name, starting on the claimed one', async () => {
    renderModal(CLAIMED, ['Mojito', 'MojitoDCL'])
    expect(screen.getByRole('tab', { name: 'UNIQUE NAME' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: 'NON-UNIQUE USERNAME' })).toHaveAttribute('aria-selected', 'false')
  })

  it('saves a NAME picked from the wallet', async () => {
    const s = renderModal(CLAIMED, ['Mojito', 'MojitoDCL'])
    await userEvent.click(screen.getByRole('button', { name: 'Claimed name' }))
    await userEvent.click(screen.getByRole('option', { name: 'MojitoDCL' }))
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    expect(s.profile.save).toHaveBeenCalledWith({ name: 'MojitoDCL' })
  })

  it('still shows the address suffix on the free-text tab — a claimed name carries none', async () => {
    renderModal(CLAIMED, ['Mojito', 'MojitoDCL'])
    await userEvent.click(screen.getByRole('tab', { name: 'NON-UNIQUE USERNAME' }))
    expect(screen.getByText('#21cf')).toBeTruthy()
  })

  it('lets you switch to a free-text name instead', async () => {
    const s = renderModal(CLAIMED, ['Mojito', 'MojitoDCL'])
    await userEvent.click(screen.getByRole('tab', { name: 'NON-UNIQUE USERNAME' }))
    const field = screen.getByLabelText('Username')
    await userEvent.clear(field)
    await userEvent.type(field, 'mojito2')
    await userEvent.click(screen.getByRole('button', { name: 'SAVE' }))
    expect(s.profile.save).toHaveBeenCalledWith({ name: 'mojito2' })
  })
})

describe('when the NAME list lands after the popup opened', () => {
  it('moves a claimed NAME\'s owner onto the picker once, and leaves a tab they chose alone', async () => {
    const s = fakeSession()
    s.profile = fakeProfileState({ data: CLAIMED, ownedNames: [] })
    const { rerender } = render(
      <SessionProvider value={s}>
        <NameEditModal onClose={vi.fn()} />
      </SessionProvider>
    )
    expect(screen.queryByRole('tab')).toBeNull()

    s.profile = { ...s.profile, ownedNames: ['Mojito', 'MojitoDCL'] }
    rerender(
      <SessionProvider value={{ ...s }}>
        <NameEditModal onClose={vi.fn()} />
      </SessionProvider>
    )
    expect(screen.getByRole('tab', { name: 'UNIQUE NAME' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('button', { name: 'Claimed name' })).toHaveTextContent('Mojito')

    await userEvent.click(screen.getByRole('tab', { name: 'NON-UNIQUE USERNAME' }))
    s.profile = { ...s.profile, ownedNames: ['Mojito', 'MojitoDCL', 'MojitoTwo'] }
    rerender(
      <SessionProvider value={{ ...s }}>
        <NameEditModal onClose={vi.fn()} />
      </SessionProvider>
    )
    expect(screen.getByRole('tab', { name: 'NON-UNIQUE USERNAME' })).toHaveAttribute('aria-selected', 'true')
  })
})

describe('without a claimed NAME', () => {
  it('offers only the free-text field, with the address suffix shown alongside', () => {
    renderModal(UNCLAIMED, [])
    expect(screen.queryByRole('tab')).toBeNull()
    expect(screen.getByLabelText('Username')).toHaveValue('bevyboy')
    // The protocol appends this; it is shown, not editable.
    expect(screen.getByText('#7cd7')).toBeTruthy()
    expect(screen.getByText('7/15')).toBeTruthy()
  })

  it('will not save a name the protocol would reject', async () => {
    const s = renderModal(UNCLAIMED, [])
    const field = screen.getByLabelText('Username')
    await userEvent.clear(field)
    await userEvent.type(field, 'bad name!')
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()
    expect(field).toHaveAttribute('aria-invalid', 'true')
    expect(s.profile.save).not.toHaveBeenCalled()
  })

  it('will not save the name you already have', () => {
    renderModal(UNCLAIMED, [])
    expect(screen.getByRole('button', { name: 'SAVE' })).toBeDisabled()
  })
})

describe('claiming, saving and failing', () => {
  it('always offers a way to buy a NAME', async () => {
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)
    renderModal(UNCLAIMED, [])
    await userEvent.click(screen.getByRole('button', { name: 'CLAIM NAME' }))
    expect(open).toHaveBeenCalledWith('https://decentraland.org/marketplace/names/claim', '_blank', 'noopener')
    open.mockRestore()
  })

  it('puts the claim panel below the actions, at the foot of the dialog', () => {
    renderModal(UNCLAIMED, [])
    const save = screen.getByRole('button', { name: 'SAVE' })
    const claim = screen.getByRole('button', { name: 'CLAIM NAME' })
    // DOCUMENT_POSITION_FOLLOWING: claim comes after save in document order.
    expect(save.compareDocumentPosition(claim) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('shows why a save failed instead of closing', () => {
    renderModal(UNCLAIMED, [], { saveError: 'failed to deploy to server.' })
    expect(screen.getByText('failed to deploy to server.')).toBeTruthy()
  })

  it('opens over the passport through the popup layer', () => {
    const s = fakeSession()
    s.profile = fakeProfileState({ data: CLAIMED, ownedNames: ['Mojito'] })
    render(
      <SessionProvider value={s}>
        <PopupHost />
      </SessionProvider>
    )
    act(() => {
      openNameEdit()
    })
    expect(screen.getByText('Edit Username')).toBeTruthy()
  })
})
