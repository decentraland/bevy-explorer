// DOMAIN: the `?systemScene=` launch gate (lib/systemScene.ts + features/gate/UntrustedLaunchGate).
//
// The parameter picks the super-user scene, which permissions.rs waves through every permission
// check and hands the whole SystemApi — so an unrecognised one is a session takeover delivered as a
// link. These cover the allowlist itself and the near-misses an attacker would actually reach for.

import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { isTrustedSystemScene, SYSTEM_SCENE } from '../lib/systemScene'
import { UntrustedLaunchGate } from '../features/gate/UntrustedLaunchGate'

describe('isTrustedSystemScene', () => {
  it('accepts the default bridge scene, with or without a trailing slash', () => {
    expect(isTrustedSystemScene(SYSTEM_SCENE)).toBe(true)
    expect(isTrustedSystemScene(`${SYSTEM_SCENE}/`)).toBe(true)
  })

  it('accepts "none" — that runs no ui scene at all, which is less privilege, not more', () => {
    expect(isTrustedSystemScene('none')).toBe(true)
    expect(isTrustedSystemScene('NONE')).toBe(true)
  })

  it('accepts a scene served from the developer\'s own machine', () => {
    expect(isTrustedSystemScene('http://localhost:8100')).toBe(true)
    expect(isTrustedSystemScene('http://127.0.0.1:8100')).toBe(true)
    expect(isTrustedSystemScene('http://[::1]:8100')).toBe(true)
    expect(isTrustedSystemScene('https://localhost:3000/some/path')).toBe(true)
  })

  it('accepts the first-party worlds, bare or as a worlds-content-server url', () => {
    for (const world of ['tortilla.dcl.eth', 'sceneviewer.dcl.eth']) {
      expect(isTrustedSystemScene(world)).toBe(true)
      expect(isTrustedSystemScene(`https://worlds-content-server.decentraland.org/world/${world}`)).toBe(true)
    }
  })

  it('rejects an arbitrary remote scene', () => {
    expect(isTrustedSystemScene('https://example.com/evil')).toBe(false)
  })

  // The near-misses: each of these passes a naive `includes`/`startsWith`/suffix check.
  it('rejects lookalikes of the trusted names', () => {
    expect(isTrustedSystemScene('https://tortilla.dcl.eth.example.com')).toBe(false)
    expect(isTrustedSystemScene('https://example.com/tortilla.dcl.eth')).toBe(false)
    expect(isTrustedSystemScene('https://localhost.example.com/evil')).toBe(false)
    expect(isTrustedSystemScene('https://example.com/?x=http://localhost')).toBe(false)
    expect(
      isTrustedSystemScene('https://worlds-content-server.decentraland.org/world/tortilla.dcl.eth/../../evil')
    ).toBe(false)
    expect(isTrustedSystemScene('https://worlds-content-server.decentraland.org.example.com/world/tortilla.dcl.eth')).toBe(
      false
    )
  })

  it('rejects a bare path, which nothing vouches for', () => {
    expect(isTrustedSystemScene('./scenes/whatever')).toBe(false)
  })
})

describe('UntrustedLaunchGate', () => {
  it('names the offending value and leads with the safe action', () => {
    render(<UntrustedLaunchGate systemScene="https://example.com/evil" onProceed={vi.fn()} />)
    expect(screen.getByText(/not trusted/i)).toBeInTheDocument()
    expect(screen.getByText('https://example.com/evil')).toBeInTheDocument()
    // Proceeding is not reachable in one click — it sits behind Advanced.
    expect(screen.queryByRole('button', { name: /continue anyway/i })).not.toBeInTheDocument()
  })

  it('only proceeds after Advanced, and only on the explicit confirm', async () => {
    const onProceed = vi.fn()
    const user = userEvent.setup()
    render(<UntrustedLaunchGate systemScene="https://example.com/evil" onProceed={onProceed} />)

    await user.click(screen.getByRole('button', { name: /advanced/i }))
    expect(onProceed).not.toHaveBeenCalled()

    await user.click(screen.getByRole('button', { name: /continue anyway/i }))
    expect(onProceed).toHaveBeenCalledTimes(1)
  })
})
