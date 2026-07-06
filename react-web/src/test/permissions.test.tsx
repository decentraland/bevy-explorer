import { describe, it, expect, vi } from 'vitest'
import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderSession, enterAsGuest } from './harness'
import { PermissionDialog } from '../features/permissions/PermissionDialog'
import type { PermissionRequestMessage } from '../engine/protocol'

const REQ: PermissionRequestMessage = {
  kind: 'permissionRequest',
  id: 7,
  ty: 'ChangeRealm',
  sceneName: 'Genesis Plaza',
  scene: 'bafkscenehash',
  realm: 'https://realm-provider.decentraland.org/main',
  additional: 'Jump to DCL Kickoff Challenge?'
}

// DOMAIN: permissions — a scene's prompt is queued, and the user's decision is posted back.
describe('permissions domain', () => {
  it('queues an incoming permission request', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    expect(h.session().permissions.pending).toHaveLength(1)
    expect(h.session().permissions.pending[0]).toMatchObject({ id: 7, ty: 'ChangeRealm' })
  })

  it('a re-sent request with the same id does not double-queue', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    h.driver.emit(REQ)
    expect(h.session().permissions.pending).toHaveLength(1)
  })

  it('resolving "once" posts the decision and clears the queue', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'once'))
    expect(h.driver.last('permissionResolve')).toEqual({
      kind: 'permissionResolve',
      id: 7,
      ty: 'ChangeRealm',
      allow: true,
      level: 'once',
      scene: 'bafkscenehash',
      realm: 'https://realm-provider.decentraland.org/main'
    })
    expect(h.session().permissions.pending).toHaveLength(0)
  })

  it('a permanent-level deny carries the chosen scope back', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'realm'))
    expect(h.driver.last('permissionResolve')).toMatchObject({ allow: false, level: 'realm' })
  })
})

describe('PermissionDialog', () => {
  it('shows the scene name + the request clause', () => {
    render(<PermissionDialog request={REQ} onResolve={vi.fn()} />)
    expect(screen.getByText('Genesis Plaza')).toBeInTheDocument()
    expect(screen.getByText(/move you to a new realm/)).toBeInTheDocument()
    expect(screen.getByText('Jump to DCL Kickoff Challenge?')).toBeInTheDocument()
  })

  it('Allow resolves with the default "once" scope', async () => {
    const onResolve = vi.fn()
    render(<PermissionDialog request={REQ} onResolve={onResolve} />)
    await userEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(onResolve).toHaveBeenCalledWith(true, 'once')
  })

  it('Deny carries the selected scope', async () => {
    const onResolve = vi.fn()
    render(<PermissionDialog request={REQ} onResolve={onResolve} />)
    await userEvent.click(screen.getByRole('radio', { name: 'Always for Realm' }))
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }))
    expect(onResolve).toHaveBeenCalledWith(false, 'realm')
  })
})
