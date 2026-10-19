import { describe, it, expect, vi, afterEach } from 'vitest'
import { act, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderSession, enterAsGuest } from './harness'
import { PermissionDialog } from '../features/permissions/PermissionDialog'
import { Sidebar } from '../features/sidebar/Sidebar'
import { PopupHost, resetPopups } from '../design'
import { fakeSession } from './harness'
import type { PermissionRequestMessage, PermissionUsedMessage } from '../engine/protocol'
import type { PermissionSubject } from '../features/session/useEngineSession'

const REQ: PermissionRequestMessage = {
  kind: 'permissionRequest',
  id: 7,
  ty: 'ChangeRealm',
  sceneName: 'Genesis Plaza',
  scene: 'bafkscenehash',
  realm: 'https://realm-provider.decentraland.org/main',
  additional: 'Jump to DCL Kickoff Challenge?'
}

const USED: PermissionUsedMessage = {
  kind: 'permissionUsed',
  ty: 'OpenUrl',
  sceneName: 'Genesis Plaza',
  scene: 'bafkscenehash',
  realm: 'https://realm-provider.decentraland.org/main',
  allowed: false
}

const SCENE_INFO = { kind: 'sceneInfo', title: 'Genesis Plaza', scene: 'bafkscenehash' } as const
const RULES = { scene: [], realm: [{ ty: 'Fetch', allow: 'Allow' as const }], global: [{ ty: 'OpenUrl', allow: 'Deny' as const }] }
// a visit answer's review subject
const SUBJECT: PermissionSubject = { ty: 'OpenUrl', sceneName: 'Genesis Plaza', scene: 'bafkscenehash', realm: USED.realm, scope: 'visit', value: 'Deny' }

// enter the world standing in SCENE_INFO's scene: answers, uses and rules are scoped to it
const enter = async (h: ReturnType<typeof renderSession>): Promise<void> => {
  await enterAsGuest(h)
  h.driver.emit(SCENE_INFO)
}

// DOMAIN: permissions — a scene's prompt is queued, and the user's decision is posted back.
describe('permissions domain', () => {
  it('queues an incoming permission request', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    expect(h.session().permissions.pending).toHaveLength(1)
    expect(h.session().permissions.pending[0]).toMatchObject({ id: 7, ty: 'ChangeRealm' })
  })

  it('a re-sent request with the same id does not double-queue', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    h.driver.emit(REQ)
    expect(h.session().permissions.pending).toHaveLength(1)
  })

  it('resolving "visit" posts the decision and clears the queue', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    expect(h.driver.last('permissionResolve')).toEqual({
      kind: 'permissionResolve',
      id: 7,
      ty: 'ChangeRealm',
      allow: true,
      level: 'visit',
      scene: 'bafkscenehash',
      realm: 'https://realm-provider.decentraland.org/main'
    })
    expect(h.session().permissions.pending).toHaveLength(0)
  })

  it('a "visit" answer is repeated for the same scene + type without queueing; other scenes still ask', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'visit'))
    h.driver.emit({ ...REQ, id: 8 })
    expect(h.session().permissions.pending).toHaveLength(0)
    expect(h.driver.last('permissionResolve')).toMatchObject({ id: 8, allow: false, level: 'visit' })
    h.driver.emit({ ...REQ, id: 9, scene: 'bafkotherhash' })
    h.driver.emit({ ...REQ, id: 10, ty: 'OpenUrl' })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9, 10])
  })

  it('a "visit" answer settles the scene\'s other queued requests for the type; other types stay', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    h.driver.emit({ ...REQ, id: 8 })
    h.driver.emit({ ...REQ, id: 9, ty: 'OpenUrl' })
    h.driver.emit({ ...REQ, id: 11, scene: 'bafkotherhash' })
    act(() => h.session().permissions.resolve(7, false, 'visit'))
    expect(h.driver.sentOf('permissionResolve').map((m) => m.id)).toEqual([7, 8])
    expect(h.driver.last('permissionResolve')).toMatchObject({ id: 8, allow: false, level: 'visit' })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9, 11])
    // the player clicked one refusal; the HUD gave the other
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.unseenDenied).toBe(1)
  })

  it('a neighbour\'s "visit" answer is kept while the player walks the current scene, and until that scene is left', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit({ ...REQ, id: 20, scene: 'bafkneighbourhash', sceneName: 'Next door' })
    act(() => h.session().permissions.resolve(20, true, 'visit'))
    // the neighbour's answer is not this scene's row
    expect(h.session().permissions.visit).toHaveLength(0)
    h.driver.emit(SCENE_INFO)
    h.driver.emit({ ...SCENE_INFO, scene: 'bafkotherhash', title: 'Across the road' })
    h.driver.emit({ ...REQ, id: 21, scene: 'bafkneighbourhash', sceneName: 'Next door' })
    expect(h.session().permissions.pending).toHaveLength(0)
    expect(h.driver.last('permissionResolve')).toMatchObject({ id: 21, allow: true, level: 'visit' })
    h.driver.emit({ ...SCENE_INFO, scene: 'bafkneighbourhash', title: 'Next door' })
    expect(h.session().permissions.visit.map((v) => v.scene)).toEqual(['bafkneighbourhash'])
    h.driver.emit(SCENE_INFO)
    h.driver.emit({ ...REQ, id: 22, scene: 'bafkneighbourhash', sceneName: 'Next door' })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([22])
  })

  it('a "visit" answer is forgotten when the player leaves the scene or reloads it', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    h.driver.emit({ kind: 'sceneInfo', title: 'Genesis Plaza', scene: 'bafkscenehash' })
    h.driver.emit({ ...REQ, id: 8 })
    expect(h.session().permissions.pending).toHaveLength(0)
    h.driver.emit({ kind: 'sceneInfo', title: 'Next door', scene: 'bafkotherhash' })
    h.driver.emit({ ...REQ, id: 9 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9])
    act(() => h.session().permissions.resolve(9, true, 'visit'))
    act(() => h.session().chat.send('/reload'))
    h.driver.emit({ ...REQ, id: 10 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([10])
  })

  it('a dismissed prompt has no row: its refusal is not counted', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'once'))
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.visit).toHaveLength(0)
    expect(h.session().permissions.triggered).toEqual([{ scene: 'bafkscenehash', ty: 'ChangeRealm', allowed: false }])
    expect(h.session().permissions.unseenDenied).toBe(0)
  })

  it('a "visit" deny is listed; the refusal the player clicked is not badged, the ones the HUD then gives are; an "Always …" answer is left to the stored rule', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'visit'))
    expect(h.session().permissions.visit).toEqual([{ ty: 'ChangeRealm', sceneName: 'Genesis Plaza', scene: 'bafkscenehash', realm: REQ.realm, allowed: false }])
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.unseenDenied).toBe(0)
    h.driver.emit({ ...REQ, id: 10 })
    expect(h.session().permissions.pending).toHaveLength(0)
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.unseenDenied).toBe(1)
    h.driver.emit({ ...REQ, id: 8, ty: 'OpenUrl' })
    act(() => h.session().permissions.resolve(8, true, 'realm'))
    expect(h.session().permissions.visit.map((v) => v.ty)).toEqual(['ChangeRealm'])
    h.driver.emit({ ...REQ, id: 9, ty: 'OpenUrl' })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9])
  })

  it('an "Always for Scene" deny: the clicked refusal is not badged, later refusals by the stored rule are', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'scene'))
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.unseenDenied).toBe(0)
    h.driver.emit({ ...USED, ty: 'ChangeRealm' })
    expect(h.session().permissions.unseenDenied).toBe(1)
  })

  it('dismissing ("once") and a stored-rule answer do not become visit answers', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'once'))
    h.driver.emit({ ...REQ, id: 8 })
    act(() => h.session().permissions.resolve(8, true, 'realm'))
    h.driver.emit({ ...REQ, id: 9 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9])
  })

  it('a review answer replaces the visit answer: "visit" is remembered here, "Always …" is not', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    act(() => h.session().permissions.setRule({ ...SUBJECT, ty: 'ChangeRealm', value: 'Allow' }, false, 'visit'))
    h.driver.emit({ ...REQ, id: 8 })
    expect(h.session().permissions.pending).toHaveLength(0)
    expect(h.driver.last('permissionResolve')).toMatchObject({ id: 8, allow: false, level: 'visit' })
    act(() => h.session().permissions.setRule({ ...SUBJECT, ty: 'ChangeRealm' }, true, 'scene'))
    h.driver.emit({ ...REQ, id: 9 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([9])
  })

  it('a withdrawn request is dropped from the queue without a decision', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    h.driver.emit({ ...REQ, id: 8 })
    h.driver.emit({ kind: 'permissionWithdrawn', id: 8 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([7])
    expect(h.driver.last('permissionResolve')).toBeUndefined()
  })

  it('a permanent-level deny carries the chosen scope back', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, false, 'realm'))
    expect(h.driver.last('permissionResolve')).toMatchObject({ allow: false, level: 'realm' })
  })
})

describe('PermissionDialog', () => {
  it('shows the scene name + the request clause', () => {
    render(<PermissionDialog kind="request" request={REQ} onResolve={vi.fn()} />)
    expect(screen.getByText('Genesis Plaza')).toBeInTheDocument()
    expect(screen.getByText(/move you to a new realm/)).toBeInTheDocument()
    expect(screen.getByText('Jump to DCL Kickoff Challenge?')).toBeInTheDocument()
  })

  it('Allow resolves with the default "This visit" scope', async () => {
    const onResolve = vi.fn()
    render(<PermissionDialog kind="request" request={REQ} onResolve={onResolve} />)
    expect(screen.getByRole('radio', { name: 'This visit' })).toBeChecked()
    await userEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(onResolve).toHaveBeenCalledWith(true, 'visit')
  })

  it('Deny carries the selected scope', async () => {
    const onResolve = vi.fn()
    render(<PermissionDialog kind="request" request={REQ} onResolve={onResolve} />)
    await userEvent.click(screen.getByRole('radio', { name: 'Always for Realm' }))
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }))
    expect(onResolve).toHaveBeenCalledWith(false, 'realm')
  })
})

// DOMAIN: permissions review — what the current scene used, and rewriting its stored rules.
describe('triggered uses + badge', () => {
  it('keeps the latest outcome per type, counts refusals a stored rule decided until the list is opened', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(USED)
    h.driver.emit(USED)
    h.driver.emit({ ...USED, ty: 'Fetch', allowed: true })
    expect(h.session().permissions.triggered.map((t) => [t.ty, t.allowed])).toEqual([
      ['OpenUrl', false],
      ['Fetch', true]
    ])
    expect(h.session().permissions.unseenDenied).toBe(1)
    act(() => h.session().permissions.markSeen())
    expect(h.session().permissions.unseenDenied).toBe(0)
    h.driver.emit({ ...USED, ty: 'Teleport' })
    expect(h.session().permissions.unseenDenied).toBe(1)
    h.driver.emit({ ...USED, allowed: true })
    expect(h.session().permissions.triggered.map((t) => [t.ty, t.allowed])).toEqual([
      ['Fetch', true],
      ['Teleport', false],
      ['OpenUrl', true]
    ])
  })

  it('is cleared with the visit answers when the player moves to another scene or reloads', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(USED)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    h.driver.emit(SCENE_INFO)
    expect(h.session().permissions.triggered).toHaveLength(1)
    expect(h.session().permissions.visit).toHaveLength(1)
    h.driver.emit({ ...SCENE_INFO, title: 'Next door', scene: 'bafkotherhash' })
    expect(h.session().permissions.triggered).toHaveLength(0)
    expect(h.session().permissions.visit).toHaveLength(0)
    h.driver.emit({ ...USED, scene: 'bafkotherhash' })
    act(() => h.session().chat.send('/reload'))
    expect(h.session().permissions.triggered).toHaveLength(0)
  })
})

describe('badge after the list was opened', () => {
  it('a refusal by a rule set after opening the list is badged; so is one after /reload', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(USED)
    act(() => h.session().permissions.markSeen())
    expect(h.session().permissions.unseenDenied).toBe(0)
    h.driver.emit({ ...REQ, ty: 'OpenUrl' })
    act(() => h.session().permissions.resolve(7, false, 'scene'))
    h.driver.emit(USED)
    expect(h.session().permissions.unseenDenied).toBe(0)
    h.driver.emit(USED)
    expect(h.session().permissions.unseenDenied).toBe(1)
    act(() => h.session().permissions.markSeen())
    act(() => h.session().chat.send('/reload'))
    h.driver.emit(USED)
    expect(h.session().permissions.unseenDenied).toBe(1)
  })

  it('a use reported for another scene neither badges nor highlights this one', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit({ ...USED, scene: 'bafkotherhash', sceneName: 'Next door' })
    expect(h.session().permissions.triggered).toHaveLength(0)
    expect(h.session().permissions.unseenDenied).toBe(0)
  })
})

describe('stored rules', () => {
  it('are fetched for the scene the player stands in on request, and dropped on leaving', async () => {
    const h = renderSession()
    await enterAsGuest(h) // no scene known yet
    act(() => h.session().permissions.loadRules())
    expect(h.driver.last('getPermissionRules')).toBeUndefined()
    h.driver.emit(SCENE_INFO)
    expect(h.driver.last('getPermissionRules')).toBeUndefined()
    act(() => h.session().permissions.loadRules())
    expect(h.driver.last('getPermissionRules')).toEqual({ kind: 'getPermissionRules', scene: 'bafkscenehash' })
    h.driver.emit({ kind: 'permissionRules', scene: 'bafkotherhash', realm: '', rules: RULES })
    expect(h.session().permissions.rules).toBeNull()
    h.driver.emit({ kind: 'permissionRules', scene: 'bafkscenehash', realm: 'https://realm/main', rules: RULES })
    expect(h.session().permissions.rules).toEqual({ scene: 'bafkscenehash', realm: 'https://realm/main', rules: RULES })
    h.driver.emit({ ...SCENE_INFO, title: 'Next door', scene: 'bafkotherhash' })
    expect(h.session().permissions.rules).toBeNull()
  })

  it('setRule posts the rule, lists a "visit" answer, and refetches', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(SCENE_INFO)
    h.driver.emit(USED)
    expect(h.session().permissions.unseenDenied).toBe(1)
    act(() => h.session().permissions.setRule(SUBJECT, true, 'visit'))
    expect(h.driver.last('permissionRule')).toEqual({
      kind: 'permissionRule',
      ty: 'OpenUrl',
      level: 'visit',
      allow: true,
      scene: 'bafkscenehash',
      realm: 'https://realm-provider.decentraland.org/main'
    })
    expect(h.session().permissions.visit).toEqual([{ ty: 'OpenUrl', sceneName: 'Genesis Plaza', scene: 'bafkscenehash', realm: USED.realm, allowed: true }])
    expect(h.session().permissions.triggered).toHaveLength(0)
    expect(h.session().permissions.unseenDenied).toBe(0)
    expect(h.driver.last('getPermissionRules')).toEqual({ kind: 'getPermissionRules', scene: 'bafkscenehash' })
  })

  it('a stored scene rule → "This visit": later requests are answered here without a prompt', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(SCENE_INFO)
    h.driver.emit({ kind: 'permissionRules', scene: 'bafkscenehash', realm: REQ.realm, rules: { ...RULES, scene: [{ ty: 'ChangeRealm', allow: 'Deny' }] } })
    const stored = h.session().permissions.rules!
    const subject: PermissionSubject = { ty: 'ChangeRealm', sceneName: 'Genesis Plaza', scene: stored.scene, realm: stored.realm, scope: 'scene', value: 'Deny' }
    act(() => h.session().permissions.clearRule(subject))
    expect(h.driver.last('permissionRule')).toMatchObject({ ty: 'ChangeRealm', level: 'scene', allow: null })
    act(() => h.session().permissions.setRule(subject, true, 'visit'))
    h.driver.emit(REQ)
    expect(h.session().permissions.pending).toHaveLength(0)
    expect(h.driver.last('permissionResolve')).toMatchObject({ id: 7, allow: true, level: 'visit' })
  })

  it('clearRule on a visit answer forgets it without an engine write; the scene asks again', async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    act(() => h.session().permissions.clearRule({ ...SUBJECT, ty: 'ChangeRealm' }))
    expect(h.driver.last('permissionRule')).toBeUndefined()
    expect(h.session().permissions.visit).toHaveLength(0)
    h.driver.emit({ ...REQ, id: 8 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([8])
  })
})

describe('reset defaults', () => {
  it("posts the reset and forgets this visit's answers and uses", async () => {
    const h = renderSession()
    await enter(h)
    h.driver.emit(REQ)
    act(() => h.session().permissions.resolve(7, true, 'visit'))
    h.driver.emit(USED)
    act(() => h.session().permissions.resetDefaults())
    expect(h.driver.last('resetPermissions')).toEqual({ kind: 'resetPermissions' })
    expect(h.session().permissions.visit).toHaveLength(0)
    expect(h.session().permissions.triggered).toHaveLength(0)
    h.driver.emit({ ...REQ, id: 8 })
    expect(h.session().permissions.pending.map((r) => r.id)).toEqual([8])
  })
})

describe('PermissionDialog review', () => {
  it('describes a visit answer and offers the same scopes, "This visit" first', async () => {
    const onRule = vi.fn()
    render(<PermissionDialog kind="review" subject={SUBJECT} initial="visit" onRule={onRule} />)
    expect(screen.getByText(/Genesis Plaza/).parentElement?.textContent).toMatch(/may not open a url in your browser this visit/)
    expect(screen.getAllByRole('radio').map((r) => r.parentElement?.textContent)).toEqual(['This visit', 'Always for Scene', 'Always for Realm', 'Always for Global'])
    expect(screen.getByRole('radio', { name: 'This visit' })).toBeChecked()
    await userEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(onRule).toHaveBeenCalledWith(true, 'visit')
  })

  it('a stored rule opens on its own level: "may not" for a scene rule, "Scenes …" for realm and global', async () => {
    const onRule = vi.fn()
    const { unmount } = render(<PermissionDialog kind="review" subject={{ ...SUBJECT, scope: 'scene' }} initial="scene" onRule={onRule} />)
    expect(screen.getByText(/Genesis Plaza/).parentElement?.textContent).toMatch(/may not open a url/)
    expect(screen.getByRole('radio', { name: 'Always for Scene' })).toBeChecked()
    unmount()
    render(<PermissionDialog kind="review" subject={{ ...SUBJECT, scope: 'global', value: 'Ask' }} initial="global" onRule={onRule} />)
    expect(screen.getByText(/Scenes ask before they open a url/)).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Always for Global' })).toBeChecked()
    await userEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(onRule).toHaveBeenCalledWith(true, 'global')
  })
})

describe('sidebar permissions button', () => {
  afterEach(resetPopups)

  it('shows the unseen refused count', () => {
    const session = fakeSession()
    session.permissions.unseenDenied = 2
    render(<Sidebar session={session} />)
    expect(screen.getByRole('button', { name: 'Scene permissions' }).textContent).toBe('2')
  })

  it('lists visit answers; a click clears the answer and opens the review dialog', async () => {
    const session = fakeSession()
    session.permissions.visit = [{ ty: 'Fetch', sceneName: 'Genesis Plaza', scene: 'bafkscenehash', realm: USED.realm, allowed: true }, { ...SUBJECT, allowed: false }]
    render(
      <>
        <Sidebar session={session} />
        <PopupHost />
      </>
    )
    await userEvent.click(screen.getByRole('button', { name: 'Scene permissions' }))
    const rows = screen.getAllByRole('button', { name: /open a url|fetch data/ }).map((b) => b.textContent)
    expect(rows).toEqual(['fetch data from a remote serveralways', 'open a url in your browsernever'])
    await userEvent.click(screen.getByRole('button', { name: /open a url in your browser/ }))
    expect(session.permissions.clearRule).toHaveBeenCalledWith(SUBJECT)
    expect(screen.queryByRole('dialog', { name: 'Scene permissions' })).toBeNull()
    expect(screen.getByRole('alertdialog', { name: 'Scene permission review' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'This visit' })).toBeChecked()
    await userEvent.click(screen.getByRole('button', { name: 'Deny' }))
    expect(session.permissions.setRule).toHaveBeenCalledWith(SUBJECT, false, 'visit')
  })

  it('opening the list marks the badge seen and loads the rules; a rule row clears the rule and opens the review on its level', async () => {
    const session = fakeSession()
    session.permissions.rules = { scene: 'bafkscenehash', realm: USED.realm, rules: RULES }
    render(
      <>
        <Sidebar session={session} />
        <PopupHost />
      </>
    )
    await userEvent.click(screen.getByRole('button', { name: 'Scene permissions' }))
    expect(session.permissions.markSeen).toHaveBeenCalledOnce()
    expect(session.permissions.loadRules).toHaveBeenCalledOnce()
    expect(screen.getByText('This realm')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: /open a url in your browser/ }))
    const subject = { ty: 'OpenUrl', sceneName: '', scene: 'bafkscenehash', realm: USED.realm, scope: 'global', value: 'Deny' }
    expect(session.permissions.clearRule).toHaveBeenCalledWith(subject)
    expect(screen.getByText(/Scenes may not open a url/)).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Always for Global' })).toBeChecked()
    await userEvent.click(screen.getByRole('button', { name: 'Allow' }))
    expect(session.permissions.setRule).toHaveBeenCalledWith(subject, true, 'global')
  })

  it('highlights the row in force for a permission the scene used: a visit answer over any rule', async () => {
    const session = fakeSession()
    session.permissions.rules = { scene: 'bafkscenehash', realm: USED.realm, rules: { ...RULES, scene: [{ ty: 'OpenUrl', allow: 'Allow' }] } }
    session.permissions.triggered = [{ ty: 'OpenUrl', allowed: false }, { ty: 'Fetch', allowed: true }]
    const { rerender } = render(<Sidebar session={session} />)
    await userEvent.click(screen.getByRole('button', { name: 'Scene permissions' }))
    const hits = (): string[] => screen.getAllByRole('button', { name: /open a url|fetch data/ }).filter((b) => b.hasAttribute('data-hit')).map((b) => b.textContent)
    // the scene rule shadows the global one; the realm Fetch rule is the one in force for Fetch
    expect(hits()).toEqual(['open a url in your browseralways', 'fetch data from a remote serveralways'])
    session.permissions.visit = [{ ...SUBJECT, allowed: false }]
    rerender(<Sidebar session={{ ...session, permissions: { ...session.permissions } }} />)
    expect(hits()).toEqual(['open a url in your browsernever', 'fetch data from a remote serveralways'])
  })

  it('closes on a pointerdown outside, the event the engine canvas lets through', async () => {
    render(<Sidebar session={fakeSession()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Scene permissions' }))
    expect(screen.getByRole('dialog', { name: 'Scene permissions' })).toBeInTheDocument()
    fireEvent.pointerDown(document.body)
    expect(screen.queryByRole('dialog', { name: 'Scene permissions' })).toBeNull()
  })

  it('Reset defaults asks first, then resets', async () => {
    const session = fakeSession()
    render(
      <>
        <Sidebar session={session} />
        <PopupHost />
      </>
    )
    await userEvent.click(screen.getByRole('button', { name: 'Scene permissions' }))
    await userEvent.click(screen.getByRole('button', { name: 'Reset defaults' }))
    expect(session.permissions.resetDefaults).not.toHaveBeenCalled()
    await userEvent.click(screen.getByRole('button', { name: 'Reset' }))
    expect(session.permissions.resetDefaults).toHaveBeenCalledOnce()
  })
})
