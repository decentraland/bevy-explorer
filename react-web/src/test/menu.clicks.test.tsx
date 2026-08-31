import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MainMenuShell } from '../features/menu/MainMenuShell'
import { ProfileChip } from '../features/menu/ProfileChip'

describe('main menu shell nav', () => {
  function shell() {
    const onNavigate = vi.fn()
    const onClose = vi.fn()
    render(
      <MainMenuShell active="settings" onNavigate={onNavigate} onClose={onClose}>
        <div>body</div>
      </MainMenuShell>
    )
    return { onNavigate, onClose }
  }

  it('clicking a non-active page navigates to it', async () => {
    const { onNavigate } = shell()
    await userEvent.click(screen.getByRole('button', { name: /Communities/ }))
    expect(onNavigate).toHaveBeenCalledWith('communities')
    await userEvent.click(screen.getByRole('button', { name: /Map/ }))
    expect(onNavigate).toHaveBeenCalledWith('map')
    await userEvent.click(screen.getByRole('button', { name: /Backpack/ }))
    expect(onNavigate).toHaveBeenCalledWith('backpack')
  })

  it('clicking the already-active page is a no-op', async () => {
    const { onNavigate } = shell()
    await userEvent.click(screen.getByRole('button', { name: /Settings/ }))
    expect(onNavigate).not.toHaveBeenCalled()
  })

  it('close button fires onClose', async () => {
    const { onClose } = shell()
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})

describe('profile chip dropdown', () => {
  function chip() {
    const onViewProfile = vi.fn()
    const onSignOut = vi.fn()
    const onExit = vi.fn()
    render(<ProfileChip name="Tester" address="0xabcdef123456" onViewProfile={onViewProfile} onSignOut={onSignOut} onExit={onExit} />)
    return { onViewProfile, onSignOut, onExit }
  }

  it('opens and routes View Profile / Sign Out / Exit', async () => {
    const { onViewProfile, onSignOut, onExit } = chip()
    await userEvent.click(screen.getByRole('button')) // the only button before open = the chip
    await userEvent.click(screen.getByRole('button', { name: /VIEW PROFILE/i }))
    expect(onViewProfile).toHaveBeenCalledTimes(1)

    await userEvent.click(screen.getByRole('button')) // reopen
    await userEvent.click(screen.getByRole('button', { name: /SIGN OUT/i }))
    expect(onSignOut).toHaveBeenCalledTimes(1)

    await userEvent.click(screen.getByRole('button', { name: /EXIT/i }))
    expect(onExit).toHaveBeenCalledTimes(1)
  })

  it('copies the wallet address', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, { clipboard: { writeText } })
    chip()
    await userEvent.click(screen.getByRole('button'))
    await userEvent.click(screen.getByTitle('Copy address'))
    expect(writeText).toHaveBeenCalledWith('0xabcdef123456')
  })
})
