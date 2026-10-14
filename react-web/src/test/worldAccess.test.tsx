import { describe, it, expect, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderSession, enterAsGuest } from './harness'
import { WorldPasswordDialog, openWorldPasswordDialog } from '../features/worldAccess/WorldPasswordDialog'
import { closeTopPopup, resetPopups } from '../design'
import type { WorldPasswordRequestMessage } from '../engine/protocol'

const REQ: WorldPasswordRequestMessage = { kind: 'worldPasswordRequest', world: 'secret-garden.dcl.eth' }

// DOMAIN: world access — the engine's password prompt for a protected world is held, and the
// player's answer is posted back.
describe('world access domain', () => {
  it('holds an incoming password prompt', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    expect(h.session().worldPassword.pending).toMatchObject({ world: 'secret-garden.dcl.eth' })
  })

  it('answering posts the password and clears the prompt', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    act(() => h.session().worldPassword.resolve('hunter2'))
    expect(h.driver.last('worldPasswordResolve')).toEqual({ kind: 'worldPasswordResolve', password: 'hunter2' })
    expect(h.session().worldPassword.pending).toBeNull()
  })

  it('giving up posts a null password', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    act(() => h.session().worldPassword.resolve(null))
    expect(h.driver.last('worldPasswordResolve')).toEqual({ kind: 'worldPasswordResolve', password: null })
  })

  it('an answer with no prompt open is dropped', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().worldPassword.resolve('x'))
    expect(h.driver.last('worldPasswordResolve')).toBeUndefined()
  })

  it('a re-ask with the reason replaces the prompt', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    h.driver.emit({ ...REQ, error: 'Incorrect password' })
    expect(h.session().worldPassword.pending).toMatchObject({ error: 'Incorrect password' })
  })

  it('a prompt for a newer travel replaces the unanswered one', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    h.driver.emit({ ...REQ, world: 'other.dcl.eth' })
    expect(h.session().worldPassword.pending).toMatchObject({ world: 'other.dcl.eth' })
    expect(h.driver.last('worldPasswordResolve')).toBeUndefined()
  })

  it('a withdrawn prompt is dropped without an answer', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    h.driver.emit({ kind: 'worldPasswordWithdrawn' })
    expect(h.session().worldPassword.pending).toBeNull()
    expect(h.driver.last('worldPasswordResolve')).toBeUndefined()
  })
})

describe('openWorldPasswordDialog', () => {
  it('closing through the handle is not an answer', () => {
    resetPopups()
    const resolve = vi.fn()
    const close = openWorldPasswordDialog(REQ, resolve)
    close()
    expect(resolve).not.toHaveBeenCalled()
  })

  it('Escape gives up', () => {
    resetPopups()
    const resolve = vi.fn()
    openWorldPasswordDialog(REQ, resolve)
    closeTopPopup()
    expect(resolve).toHaveBeenCalledWith(null)
  })
})

describe('WorldPasswordDialog', () => {
  it('names the world and shows the refusal reason', () => {
    render(<WorldPasswordDialog request={{ ...REQ, error: 'Incorrect password' }} onResolve={vi.fn()} />)
    expect(screen.getByText('secret-garden.dcl.eth')).toBeInTheDocument()
    expect(screen.getByRole('alert')).toHaveTextContent('Incorrect password')
  })

  it('Enter submits the typed password', async () => {
    const onResolve = vi.fn()
    render(<WorldPasswordDialog request={REQ} onResolve={onResolve} />)
    expect(screen.getByRole('button', { name: 'Enter' })).toBeDisabled()
    await userEvent.type(screen.getByLabelText('Password'), 'hunter2{Enter}')
    expect(onResolve).toHaveBeenCalledWith('hunter2')
  })

  it('Cancel gives up', async () => {
    const onResolve = vi.fn()
    render(<WorldPasswordDialog request={REQ} onResolve={onResolve} />)
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(onResolve).toHaveBeenCalledWith(null)
  })
})
