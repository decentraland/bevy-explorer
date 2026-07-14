// A fake bridge "scene" that answers the protocol on the BroadcastChannel, so the
// React HUD runs fully standalone (no engine, no wasm) under `?mock=1`. In the real
// app this is the super-user SDK7 bridge scene forwarding to SystemApi — the
// page-side BridgeClient does not change at all.

import {
  BRIDGE_CHANNEL,
  type Envelope,
  type PageToScene,
  type Profile,
  type SceneToPage,
  type Setting,
  type Wearable
} from './protocol'

// A fully-populated passport for the mock, so the React passport shows every section.
function richProfile(address: string, name: string, isGuest: boolean): Profile {
  return {
    address,
    name,
    picture: 'https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png',
    bodyImage: `https://picsum.photos/seed/${encodeURIComponent(address)}/360/760`,
    hasClaimedName: !name.includes('#'),
    isGuest,
    description: 'Exploring Decentraland one plaza at a time. 🌅 DCL citizen since 2022.',
    links: [
      { title: 'Twitter', url: 'https://twitter.com' },
      { title: 'Discord', url: 'https://discord.com' }
    ],
    mutuals: 30,
    badges: Array.from({ length: 8 }, (_, i) => ({ id: `b${i}`, name: `Badge ${i + 1}`, tier: ['bronze', 'silver', 'gold'][i % 3], image: `https://picsum.photos/seed/badge${i}/96/96` })),
    photos: Array.from({ length: 6 }, (_, i) => `https://picsum.photos/seed/photo${i}/300/300`),
    info: {
      gender: 'Male',
      birthdate: '26/11/1991',
      pronouns: 'He / Him',
      relationship: 'Single',
      language: 'Persian',
      profession: 'IT',
      employment: 'Chilling',
      hobby: 'games.movie.party',
      realName: 'mohammad'
    }
  }
}

// Nearby roster — shared by the `members` stream and getUserProfile (so a fetched
// passport resolves a real name from the clicked sender's address).
const MOCK_NEARBY = [
  { address: '0x5854cce95d5e25817b41f4c41f06b695a83bc495', name: 'Mojito', picture: 'https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png' },
  { address: '0x6723dcb07f3ca735223cd1c0acfa62dd994a1bb4', name: 'Sharknado', picture: 'https://profile-images.decentraland.org/entities/bafkreie5bpho47gnh3jrfxoezwc4pxffup4cmmhmdxsmpf3oslopxb4enm/face.png' },
  { address: '0x1e105bb213754519903788022b962fe2b9c4b263', name: 'Pravus', picture: 'https://profile-images.decentraland.org/entities/bafkreig4xay5oxgbf75hwkjefx5hgdcdvm6a4tnpiisltslxf4jtajkbyq/face.png' },
  { address: '0x9f8c2a1b4d6e7f0a3b5c8d9e1f2a4b6c8d0e1f23', name: 'Johnny' },
  { address: '0x3a1b2c4d5e6f7081920a3b4c5d6e7f8091a2b3c4', name: 'Clara' },
  { address: '0x77a0b1c2d3e4f5061728394a5b6c7d8e9f001122', name: 'SpottyGoat' },
  { address: '0xc0ffee254729296a45a3885639ac7e10f9d54979', name: '' }
]

const mockCommunities = [
  { id: 'c1', name: 'Decentraland Foundation', description: 'The official Decentraland Foundation community. Stay up to date with events, releases and everything happening across the metaverse.', thumbnail: 'https://picsum.photos/seed/dcl/540/360', membersCount: 1242, role: 'member', ownerName: 'DCLOfficial', privacy: 'public' },
  { id: 'c2', name: 'Chiri Storytelling', description: "This space brings to life Chiri's adventures. Gathering cozy Social Gamers, Creators, Digi Fashion lovers, Virtual Citizens & more!", thumbnail: '', membersCount: 374, role: 'none', ownerName: 'Chiri', privacy: 'private' },
  { id: 'c3', name: 'VTATV', description: 'Virtual television and live shows broadcast from inside Decentraland.', thumbnail: '', membersCount: 186, role: 'none', ownerName: 'VTATV' },
  { id: 'c4', name: 'ABC Decentraland', description: 'Adventures and education for newcomers to the metaverse.', thumbnail: '', membersCount: 436, role: 'none', ownerName: 'BillyTeacoin' },
  { id: 'c5', name: 'Toxic Events', description: 'The wildest parties and events in DCL.', thumbnail: '', membersCount: 601, role: 'member', ownerName: 'ToxicWaifu' },
  { id: 'c6', name: 'Last Slice Creators', description: 'A community of pizza-loving 3D creators.', thumbnail: '', membersCount: 164, role: 'none', ownerName: 'Lastraum' },
  { id: 'c7', name: 'SheFi', description: 'Empowering women in web3 and DeFi.', thumbnail: '', membersCount: 310, role: 'none', ownerName: 'msjoedor' },
  { id: 'c8', name: 'Community Building DCL', description: 'Helping communities grow in Decentraland.', thumbnail: '', membersCount: 639, role: 'owner', ownerName: 'TheCryptKeeper' }
]

const RARITIES = ['base', 'common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic', 'unique', 'exotic']
const thumb = (urn: string): string => `https://peer.decentraland.org/lambdas/collections/contents/${urn}/thumbnail`
// Real base-avatar wearables so the mock grid shows actual thumbnails (catalyst).
const BASE: { name: string; category: string; label: string }[] = [
  { name: 'casual_hair_01', category: 'hair', label: 'Casual Hair' },
  { name: 'cornrows', category: 'hair', label: 'Cornrows' },
  { name: 'hair_anime_01', category: 'hair', label: 'Anime Hair' },
  { name: 'eyebrows_00', category: 'eyebrows', label: 'Eyebrows' },
  { name: 'mouth_00', category: 'mouth', label: 'Mouth' },
  { name: 'beard', category: 'facial_hair', label: 'Beard' },
  { name: 'granpa_beard', category: 'facial_hair', label: "Grandpa Beard" },
  { name: 'f_sweater', category: 'upper_body', label: 'Sweater' },
  { name: 'm_greenhoodie', category: 'upper_body', label: 'Green Hoodie' },
  { name: 'blue_tshirt', category: 'upper_body', label: 'Blue T-Shirt' },
  { name: 'sport_jacket', category: 'upper_body', label: 'Sport Jacket' },
  { name: 'f_jeans', category: 'lower_body', label: 'Jeans' },
  { name: 'f_brown_trousers', category: 'lower_body', label: 'Brown Trousers' },
  { name: 'basketball_shorts', category: 'lower_body', label: 'Basketball Shorts' },
  { name: 'sneakers', category: 'feet', label: 'Sneakers' },
  { name: 'bun_shoes', category: 'feet', label: 'Bun Shoes' },
  { name: 'm_greenflipflops', category: 'feet', label: 'Flip Flops' },
  { name: 'hat', category: 'hat', label: 'Hat' },
  { name: 'bandana', category: 'hat', label: 'Bandana' },
  { name: 'black_sun_glasses', category: 'eyewear', label: 'Sunglasses' },
  { name: 'piratepatch', category: 'eyewear', label: 'Pirate Patch' },
  { name: 'blue_bandana', category: 'mask', label: 'Blue Bandana' },
  { name: 'pink_gem_earring', category: 'earring', label: 'Pink Gem Earring' },
  { name: 'Thunder_earring', category: 'earring', label: 'Thunder Earring' }
]
const mockWearables: Wearable[] = BASE.map((b, i) => {
  const urn = `urn:decentraland:off-chain:base-avatars:${b.name}`
  return {
    urn,
    name: b.label,
    rarity: RARITIES[i % RARITIES.length],
    category: b.category,
    thumbnail: thumb(urn),
    equipped: i % 6 === 0
  }
})

const v = (name: string): { name: string; description: string } => ({ name, description: '' })

// Stateful so the mock's controls actually move when changed (setSetting → re-emit).
const mockSettings: Setting[] = [
  { name: 'fullscreen', category: 'general', description: 'Run in fullscreen mode.', minValue: 0, maxValue: 1, namedVariants: [v('Off'), v('On')], value: 1, default: 1, stepSize: 1 },
  { name: 'resolution', category: 'general', description: '', minValue: 0, maxValue: 2, namedVariants: [v('1280 × 720'), v('1920 × 1080'), v('2560 × 1440')], value: 1, default: 1, stepSize: 1 },
  { name: 'graphics_quality', category: 'graphics', description: 'Overall visual quality.', minValue: 0, maxValue: 3, namedVariants: [v('Low'), v('Medium'), v('High'), v('Ultra')], value: 2, default: 1, stepSize: 1 },
  { name: 'fps_limit', category: 'graphics', description: 'Cap the frame rate.', minValue: 30, maxValue: 144, namedVariants: [], value: 60, default: 60, stepSize: 1 },
  { name: 'master_volume', category: 'audio', description: '', minValue: 0, maxValue: 100, namedVariants: [], value: 80, default: 100, stepSize: 1 },
  { name: 'voice_chat', category: 'audio', description: 'Enable voice chat.', minValue: 0, maxValue: 1, namedVariants: [v('Off'), v('On')], value: 1, default: 1, stepSize: 1 }
]

interface MockOptions {
  /** Simulate a returning user (reuse-login flow) vs a fresh user. */
  hasPreviousLogin: boolean
  /** Simulated user id for the returning-user case. */
  userId: string
  /** Latency for simulated calls, ms. */
  latency: number
}

const DEFAULTS: MockOptions = {
  hasPreviousLogin:
    new URLSearchParams(location.search).get('previousLogin') === '1',
  userId: '0xmock00000000000000000000000000000000beef',
  latency: 600
}

// In ?mock=1&previousLogin=1, seed a fake same-domain SSO identity so the session's
// localStorage read lights up the "welcome back" reuse flow (no real auth site in mock).
// The 0xmock… address is deliberately non-hex so sso.ts hides it from real engine sessions.
function seedMockSso(o: MockOptions): void {
  const key = `single-sign-on-${o.userId}`
  if (o.hasPreviousLogin) {
    if (!localStorage.getItem(key)) {
      localStorage.setItem(
        key,
        JSON.stringify({
          ephemeralIdentity: { address: '0xeeee000000000000000000000000000000000001', publicKey: '0x04', privateKey: '0x' + '11'.repeat(32) },
          expiration: new Date(Date.now() + 30 * 24 * 3600 * 1000).toISOString(),
          authChain: [
            { type: 'SIGNER', payload: o.userId, signature: '' },
            { type: 'ECDSA_EPHEMERAL', payload: 'Decentraland Login\nEphemeral address: 0xeeee\nExpiration: ', signature: '0xabc' }
          ]
        })
      )
    }
  } else {
    localStorage.removeItem(key)
  }
}

export function startMockBridge(opts: Partial<MockOptions> = {}): () => void {
  const o = { ...DEFAULTS, ...opts }
  seedMockSso(o)
  // Stateful so markNotificationsRead persists across reopens (like the real service).
  const mockNow = 1_700_000_000_000
  // Shapes mirror the real notifications service: friendship notifications carry the other user
  // under metadata.sender (name + avatar), with no metadata.title.
  const mockNotifications = [
    { id: 'n1', type: 'social_service_friendship_accepted', read: false, timestamp: new Date(mockNow).toISOString(), metadata: { sender: { address: '0x5854cce95d5e25817b41f4c41f06b695a83bc495', name: 'Mojito', profileImageUrl: 'https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png' } } },
    { id: 'n2', type: 'social_service_friendship_request', read: false, timestamp: new Date(mockNow - 1800_000).toISOString(), metadata: { sender: { address: '0x6723dcb07f3ca735223cd1c0acfa62dd994a1bb4', name: 'Sharknado', profileImageUrl: 'https://profile-images.decentraland.org/entities/bafkreie5bpho47gnh3jrfxoezwc4pxffup4cmmhmdxsmpf3oslopxb4enm/face.png' } } },
    { id: 'n3', type: 'item_sold', read: false, timestamp: new Date(mockNow - 3600_000).toISOString(), metadata: { title: 'Item sold', description: 'Your “Sunset Hoodie” sold for 120 MANA.', image: 'https://profile-images.decentraland.org/entities/bafkreie5bpho47gnh3jrfxoezwc4pxffup4cmmhmdxsmpf3oslopxb4enm/face.png' } },
    { id: 'n4', type: 'event_started', read: true, timestamp: new Date(mockNow - 86_400_000).toISOString(), metadata: { title: 'Event live', description: 'Music Festival is happening now in Genesis Plaza.' } },
    // Title-less types (server sends only structured fields) — formatted client-side in the panel.
    { id: 'n5', type: 'community_post_added', read: false, timestamp: new Date(mockNow - 2400_000).toISOString(), metadata: { communityName: 'Toxic Events', communityId: 'c5', thumbnailUrl: 'https://picsum.photos/seed/dcl/80/80' } },
    { id: 'n6', type: 'credits_reminder_claim_credits', read: false, timestamp: new Date(mockNow - 5400_000).toISOString(), metadata: {} }
  ]
  // Mock camera-reel gallery — ~3-day spacing spans two months (exercises month grouping).
  // dateTime is a unix-seconds string, like the real compact endpoint. Mutable (delete splices).
  const DAY = 86_400_000
  const mockGallery = Array.from({ length: 14 }, (_, i) => ({
    id: `g${i}`,
    url: `https://picsum.photos/seed/reel${i}/1200/800`,
    thumbnailUrl: `https://picsum.photos/seed/reel${i}/400/300`,
    isPublic: i % 2 === 0,
    dateTime: String(Math.floor((mockNow - i * 3 * DAY) / 1000))
  }))
  const ch = new BroadcastChannel(BRIDGE_CHANNEL)
  const wait = (ms: number): Promise<void> =>
    new Promise((r) => setTimeout(r, ms))

  const reply = (msg: SceneToPage): void => {
    ch.postMessage({ to: 'page', msg } satisfies Envelope)
  }

  // An in-flight loginNew approval (so loginCancel/logout can abort it).
  let pendingAuth: { id: string; timer: ReturnType<typeof setTimeout> } | null = null

  // No engine in mock mode, so stand in for the engine's 'Cancel' system action: relay a DOM Escape
  // as the same message the real bridge sends from getSystemActionStream (closes the topmost popup).
  const onEscape = (e: KeyboardEvent): void => {
    if (e.key === 'Escape') reply({ kind: 'systemAction', action: 'Cancel' })
  }
  window.addEventListener('keydown', onEscape)

  // Simulate the engine spawning the player + loading the spawn scene after a
  // successful login: player-ready, then a scene-asset countdown, then "done".
  const spawnPlayer = (): void => {
    setTimeout(() => {
      reply({ kind: 'event', name: 'playerReady' })
      reply({ kind: 'chatVisibility', open: true })
      // Two-step countdown then done — kept short so a throttled/backgrounded tab
      // (which clamps setTimeout) still reaches the world in ~2s instead of crawling.
      const steps = [564, 60, 0]
      const tick = (i: number): void => {
        const pending = steps[i]
        reply({
          kind: 'sceneLoading',
          state: {
            visible: pending > 0,
            realmConnected: true,
            title: 'Genesis Plaza',
            pendingAssets: pending > 0 ? pending : null
          }
        })
        if (pending > 0) setTimeout(() => tick(i + 1), 250)
      }
      tick(0)
    }, 500)
    // Fake nearby roster → drives the "Nearby · N" count + members list, and lets
    // chat bubbles resolve sender addresses to names.
    setTimeout(
      () =>
        reply({
          kind: 'members',
          members: MOCK_NEARBY
        }),
      1400
    )

    // ?simhover=N seeds N world-hover prompts so the radial cursor tooltips are visible/verifiable in
    // ?mock=1 (the real engine hover stream isn't mocked). React positions them at the live DOM cursor,
    // so we only send the action list — no coordinates. One is disabled (camera-distance gated →
    // shows the camera glyph; see pointer.test.tsx for the player-distance / walking-glyph variant).
    const simHover = Number(new URLSearchParams(location.search).get('simhover') ?? 0)
    if (simHover > 0) {
      const sample = [
        { button: 0, text: 'Show Profile', enabled: true },
        { button: 1, text: 'Open', enabled: true },
        { button: 2, text: 'Inspect', enabled: true },
        { button: 8, text: 'Jump', enabled: true },
        { button: 4, text: 'Grab', enabled: false, tooFarReason: 'camera' as const },
        { button: 10, text: 'Use', enabled: true },
        { button: 11, text: 'Activate', enabled: true }
      ].slice(0, Math.min(simHover, 7))
      setTimeout(() => reply({ kind: 'hover', actions: sample }), 1500)
    }

    // Fake friends + requests for the React friends panel.
    setTimeout(
      () =>
        reply({
          kind: 'friends',
          available: true,
          friends: [
            { address: '0x5854cce95d5e25817b41f4c41f06b695a83bc495', name: 'Mojito', status: 'online', picture: 'https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png' },
            { address: '0x6723dcb07f3ca735223cd1c0acfa62dd994a1bb4', name: 'Sharknado', status: 'online' },
            { address: '0x1e105bb213754519903788022b962fe2b9c4b263', name: 'Pravus', status: 'away' },
            { address: '0x3a1b2c4d5e6f7081920a3b4c5d6e7f8091a2b3c4', name: 'Clara#1l0u', status: 'offline' }
          ],
          received: [
            { id: 'r1', address: '0x77a0b1c2d3e4f5061728394a5b6c7d8e9f001122', name: 'SpottyGoat', message: 'Met you at the plaza!' }
          ],
          sent: [{ id: 's1', address: '0x9f8c2a1b4d6e7f0a3b5c8d9e1f2a4b6c8d0e1f23', name: 'Johnny' }],
          blocked: []
        }),
      1600
    )

    // Settings snapshot for the React settings panel.
    setTimeout(() => reply({ kind: 'settings', settings: mockSettings }), 1700)

    // Drip a couple of fake nearby messages so the chat isn't empty.
    const samples = [
      ['0x5854cce95d5e25817b41f4c41f06b695a83bc495', 'gm everyone 👋'],
      ['0x6723dcb07f3ca735223cd1c0acfa62dd994a1bb4', 'anyone going to the event?'],
      ['0x1e105bb213754519903788022b962fe2b9c4b263', 'this plaza looks great']
    ]
    samples.forEach(([sender, message], i) =>
      setTimeout(
        () => reply({ kind: 'chat', chat: { sender, message, channel: 'Nearby' } }),
        2500 + i * 2200
      )
    )

    // ?perm=1 → fire a sample scene permission prompt so the dialog is exercisable in the mock.
    if (new URLSearchParams(location.search).get('perm') === '1') {
      setTimeout(
        () =>
          reply({
            kind: 'permissionRequest',
            id: 1,
            ty: 'ChangeRealm',
            sceneName: 'Genesis Plaza',
            scene: 'bafkreigenesisplazahash',
            realm: 'https://realm-provider.decentraland.org/main',
            additional: 'Jump to DCL Kickoff Challenge?'
          }),
        3000
      )
    }
  }

  ch.onmessage = async (e: MessageEvent<Envelope>) => {
    const env = e.data
    if (env?.to !== 'scene') return
    const msg: PageToScene = env.msg
    await wait(o.latency)

    if (msg.kind === 'sendChat') {
      // Echo the local player's message back (the engine would broadcast it).
      reply({ kind: 'chat', chat: { sender: 'You', message: msg.message, channel: msg.channel } })
      return
    }

    if (msg.kind === 'navAction') return // no scene menus in the mock
    if (msg.kind === 'friendAction') return // social actions are no-ops in the mock
    if (msg.kind === 'getSettings') {
      reply({ kind: 'settings', settings: mockSettings })
      return
    }
    if (msg.kind === 'setSetting') {
      const s = mockSettings.find((x) => x.name === msg.name)
      if (s) s.value = msg.value
      reply({ kind: 'settings', settings: mockSettings })
      return
    }
    if (msg.kind === 'getEmotes') {
      const names = ['Hands Air', 'Wave', 'Fist Pump', 'Dance', 'Raise Hand', 'Clap', 'Money', 'Kiss', 'Head Explode', 'Shrug']
      reply({
        kind: 'emotes',
        // The 10 default emotes are all 'base' rarity (matches the real relay). Custom
        // equipped emotes would carry their own rarity from the catalog. No `thumbnail`
        // field — mirrors the scene relay so we validate URN-derived thumbnails too.
        emotes: names.map((name, slot) => {
          const urn = `urn:decentraland:off-chain:base-emotes:${name.toLowerCase().replace(/ /g, '')}`
          return { slot, urn, name, rarity: 'base' }
        })
      })
      return
    }
    if (msg.kind === 'triggerEmote') return // no-op in the mock
    if (msg.kind === 'equipEmote') return // no-op in the mock
    if (msg.kind === 'getWearables') {
      reply({ kind: 'wearables', wearables: mockWearables })
      return
    }
    if (msg.kind === 'equip') {
      const set = new Set(msg.urns)
      for (const w of mockWearables) w.equipped = set.has(w.urn)
      reply({ kind: 'wearables', wearables: mockWearables })
      return
    }
    if (msg.kind === 'setMic') {
      reply({ kind: 'mic', enabled: msg.enabled, available: true })
      return
    }
    if (msg.kind === 'getMap') {
      reply({ kind: 'mapState', x: 0, y: 0 })
      return
    }
    if (msg.kind === 'engineViewport') {
      // No engine in mock mode — nothing to render into the cutout.
      return
    }
    if (msg.kind === 'previewAvatar') {
      // No engine in mock mode — avatar preview has nothing to render.
      return
    }
    if (msg.kind === 'teleport') {
      reply({ kind: 'mapState', x: msg.x, y: msg.y })
      return
    }
    if (msg.kind === 'changeRealm') return // no realm switching in the mock
    if (msg.kind === 'permissionResolve') return // no engine to apply the decision in the mock
    if (msg.kind === 'getCommunities') {
      reply({ kind: 'communities', communities: mockCommunities })
      return
    }
    if (msg.kind === 'createCommunity') {
      mockCommunities.unshift({
        id: `new-${mockCommunities.length}`,
        name: msg.name,
        description: msg.description,
        thumbnail: '',
        membersCount: 1,
        role: 'owner',
        ownerName: 'You',
        privacy: msg.privacy
      })
      reply({ kind: 'communities', communities: mockCommunities })
      return
    }
    if (msg.kind === 'joinCommunity') {
      const c = mockCommunities.find((x) => x.id === msg.id)
      if (c) {
        c.role = 'member'
        c.membersCount += 1
      }
      reply({ kind: 'communities', communities: mockCommunities })
      return
    }
    if (msg.kind === 'leaveCommunity') {
      const c = mockCommunities.find((x) => x.id === msg.id)
      if (c) {
        c.role = 'none'
        c.membersCount = Math.max(0, c.membersCount - 1)
      }
      reply({ kind: 'communities', communities: mockCommunities })
      return
    }
    if (msg.kind === 'getCommunityDetail') {
      const memberNames = ['DCLOfficial', 'KazeNoKai', 'METAWOLF', 'laurenmae', 'Eax', 'Thund', 'aixa', 'Kimbo', 'BayBackner', 'Rochyou', 'Soultasium', 'franfranfran', 'olavra', 'Meshroom']
      const addr = (i: number): string => `0x${(i + 1).toString(16).padStart(40, '0')}`
      const day = 24 * 60 * 60 * 1000
      reply({
        kind: 'communityDetail',
        id: msg.id,
        members: memberNames.map((name, i) => ({
          address: addr(i),
          name,
          role: i === 0 ? 'owner' : i < 3 ? 'moderator' : 'member',
          picture: `https://i.pravatar.cc/80?u=${name}`,
          hasClaimedName: true,
          isFriend: i % 5 === 0
        })),
        posts: [
          { id: 'p1', author: 'Kimbo', authorAddress: addr(7), authorPicture: 'https://i.pravatar.cc/80?u=Kimbo', text: 'It was TIME for an update. See you all in the new Plaza. Enjoy!', timestamp: Date.now() - 2 * day, likes: 10 },
          { id: 'p2', author: 'Kimbo', authorAddress: addr(7), authorPicture: 'https://i.pravatar.cc/80?u=Kimbo', text: 'EPIC Launch Party happening now in the theatre!', timestamp: Date.now() - 16 * day, likes: 4 },
          { id: 'p3', author: 'Kimbo', authorAddress: addr(7), authorPicture: 'https://i.pravatar.cc/80?u=Kimbo', text: 'Hey everyone! Happy Epic and Mobile Launch Day!!! Please help us spread the word and get ready to celebrate together on April 2nd! It will be EPIC.', timestamp: Date.now() - 18 * day, likes: 1 }
        ],
        places: [
          { id: 'pl1', title: 'Music Festival Main Stage', thumbnail: 'https://picsum.photos/seed/mfest/420/260', positions: '-66,56', likeRate: 1 }
        ],
        photos: Array.from({ length: 9 }, (_, i) => ({
          id: `ph${i}`,
          url: `https://picsum.photos/seed/dclph${i}/600/600`,
          thumbnail: `https://picsum.photos/seed/dclph${i}/300/300`
        })),
        events: [
          { id: 'e1', name: 'Watch scary movies with Cult Horror Club', thumbnail: 'https://picsum.photos/seed/horror1/200/120', startsAt: Date.now() + 1 * day },
          { id: 'e2', name: 'Pride Sound Set with Artist X', thumbnail: 'https://picsum.photos/seed/pride/200/120', startsAt: Date.now() + 1 * day + 6 * 60 * 60 * 1000 },
          { id: 'e3', name: 'Join DEEJAY FAMILY at Takeover Tuesdays', thumbnail: 'https://picsum.photos/seed/takeover/200/120', startsAt: Date.now() + 4 * day },
          { id: 'e4', name: 'PRIDE: Join Watch Party Wednesdays', thumbnail: 'https://picsum.photos/seed/watch/200/120', startsAt: Date.now() + 5 * day }
        ]
      })
      return
    }
    if (msg.kind === 'getNotifications') {
      reply({ kind: 'notifications', notifications: mockNotifications })
      return
    }
    if (msg.kind === 'markNotificationsRead') {
      // Persist read state in the mock so reopening reflects it (mirrors the real service).
      for (const n of mockNotifications) if (msg.ids.includes(n.id)) n.read = true
      return
    }
    if (msg.kind === 'getGallery') {
      reply({ kind: 'gallery', photos: mockGallery, current: mockGallery.length, max: 500 })
      return
    }
    if (msg.kind === 'getGalleryPhoto') {
      reply({
        kind: 'galleryPhoto',
        id: msg.id,
        meta: {
          userName: o.hasPreviousLogin ? 'Mojito' : 'Guest',
          userAddress: o.userId,
          sceneName: 'Genesis Plaza',
          x: -9,
          y: 14,
          realm: 'main',
          people: MOCK_NEARBY.slice(0, 3).map((m) => ({ address: m.address, name: m.name || 'Guest', isGuest: false }))
        }
      })
      return
    }
    if (msg.kind === 'deleteGalleryPhoto') {
      const i = mockGallery.findIndex((p) => p.id === msg.id)
      if (i >= 0) mockGallery.splice(i, 1)
      reply({ kind: 'gallery', photos: mockGallery, current: mockGallery.length, max: 500 })
      return
    }
    if (msg.kind === 'getProfile') {
      reply({
        kind: 'profile',
        profile: richProfile(o.userId, o.hasPreviousLogin ? 'Mojito' : 'Guest#beef', !o.hasPreviousLogin)
      })
      return
    }
    if (msg.kind === 'getUserProfile') {
      // Resolve a real name from the nearby roster (real engine gets it from the catalyst).
      const member = MOCK_NEARBY.find((m) => m.address.toLowerCase() === msg.address.toLowerCase())
      const name = member?.name || `${msg.address.slice(0, 6)}…${msg.address.slice(-4)}`
      reply({ kind: 'userProfile', address: msg.address, profile: richProfile(msg.address, name, false) })
      return
    }

    switch (msg.method) {
      case 'getPreviousLogin':
        reply({
          kind: 'rpc:res',
          id: msg.id,
          ok: true,
          value: { userId: o.hasPreviousLogin ? o.userId : null }
        })
        return
      case 'loginPrevious':
        reply({
          kind: 'rpc:res',
          id: msg.id,
          ok: true,
          value: { success: true, error: '' }
        })
        spawnPlayer()
        return
      case 'loginGuest':
        reply({ kind: 'rpc:res', id: msg.id, ok: true })
        spawnPlayer()
        return
      case 'loginIdentity':
        // Same-domain SSO hand-off: the engine would finalize the wallet from the identity.
        reply({ kind: 'rpc:res', id: msg.id, ok: true })
        spawnPlayer()
        return
      case 'loginNew':
        // Remote-wallet flow: code first, then approval after a beat (long enough to see the
        // verification panel; the real flow waits on the user's external browser).
        reply({ kind: 'loginCode', code: '42' })
        pendingAuth = {
          id: msg.id,
          timer: setTimeout(() => {
            pendingAuth = null
            reply({ kind: 'rpc:res', id: msg.id, ok: true, value: { success: true, error: '' } })
            spawnPlayer()
          }, 2500)
        }
        return
      case 'loginCancel':
      case 'logout':
        // Mid-loginNew this stops the pending approval, mirroring the real flow (the engine
        // drops its login task and the relay resolves the rpc as cancelled).
        if (pendingAuth != null) {
          clearTimeout(pendingAuth.timer)
          reply({
            kind: 'rpc:res',
            id: pendingAuth.id,
            ok: true,
            value: { success: false, error: 'cancelled' }
          })
          pendingAuth = null
        }
        reply({ kind: 'rpc:res', id: msg.id, ok: true })
        return
    }
  }

  return () => {
    window.removeEventListener('keydown', onEscape)
    ch.close()
  }
}
