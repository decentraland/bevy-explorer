import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ProfilePassport } from '../features/profile/ProfilePassport'
import type { Emote, Profile, Wearable } from '../engine/protocol'

const profile: Profile = {
  address: '0xkurd000000000000000000000000000000006b635',
  name: 'kurd',
  picture: 'k.png',
  hasClaimedName: true,
  isGuest: false,
  description: 'old gamer in dcl',
  mutuals: 30,
  links: [{ title: 'x account', url: 'https://x.com/kurd' }],
  badges: [
    { id: 'festive', name: 'Festive Trail', description: 'Followed the trail', image: 'https://x/festive.png', unlockedAt: Date.UTC(2024, 11, 4, 14, 16, 34) },
    {
      id: 'traveler',
      name: 'Traveler',
      description: 'Scenes explored',
      tier: 'Starter',
      unlockedAt: Date.UTC(2024, 10, 15, 12),
      progress: { done: 5, target: 50 },
      tiers: [
        { id: 'traveler-starter', name: 'Starter', description: '1 scene explored', unlockedAt: Date.UTC(2024, 10, 15, 12) },
        { id: 'traveler-bronze', name: 'Bronze', description: '50 scenes explored' }
      ]
    },
    { id: 'voyager', name: 'Vertical Voyager', description: 'Reached 500m', image: 'https://x/voyager.png', locked: true }
  ],
  info: { gender: 'Male', realName: 'mohammad', language: 'Persian' }
}

describe('profile passport', () => {
  it('renders the overview: name, about, fields, links, mutuals', () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    expect(screen.getByText('kurd')).toBeInTheDocument()
    expect(screen.getByText('old gamer in dcl')).toBeInTheDocument()
    expect(screen.getByText('mohammad')).toBeInTheDocument() // Real Name field value
    expect(screen.getByText('30 Mutual')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /x account/i })).toHaveAttribute('href', 'https://x.com/kurd')
  })

  it.each([
    ['none', 'ADD FRIEND', 'onAddFriend'],
    ['friend', 'FRIEND', 'onUnfriend'],
    ['requested', 'CANCEL REQUEST', 'onCancelRequest'],
    ['incoming', 'ACCEPT FRIEND', 'onAcceptRequest'],
    ['blocked', 'UNBLOCK', 'onUnblock']
  ] as const)('the friendship button for %s reads %s and calls %s', async (relationship, label, handler) => {
    const fn = vi.fn()
    render(<ProfilePassport profile={profile} relationship={relationship} {...{ [handler]: fn }} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: new RegExp(`^${label}`) }))
    expect(fn).toHaveBeenCalledWith(profile.address)
  })

  it('hides the friend action on your own passport (isSelf)', () => {
    render(<ProfilePassport profile={profile} isSelf onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
  })

  it('uses the full-body snapshot as the avatar when present', () => {
    render(<ProfilePassport profile={{ ...profile, bodyImage: 'https://x/body.png' }} onClose={vi.fn()} />)
    expect(screen.getAllByRole('img').some((i) => i.getAttribute('src') === 'https://x/body.png')).toBe(true)
  })

  it('close button closes', async () => {
    const onClose = vi.fn()
    render(<ProfilePassport profile={profile} onClose={onClose} />)
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('the Overview row shows unlocked badges only', () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    expect(screen.getByAltText('Festive Trail')).toBeInTheDocument()
    expect(screen.queryByAltText('Vertical Voyager')).not.toBeInTheDocument()
  })

  describe('Badges tab', () => {
    const open = async (isSelf = false) => {
      render(<ProfilePassport profile={profile} isSelf={isSelf} onClose={vi.fn()} />)
      await userEvent.click(screen.getByRole('tab', { name: 'BADGES' }))
    }

    it('lists every badge as a card with its unlock month, a dash while locked', async () => {
      await open()
      const cards = screen.getAllByRole('button').filter((b) => b.hasAttribute('aria-pressed'))
      expect(cards.map((c) => c.textContent)).toEqual(['Festive TrailDec. 2024', 'Traveler · StarterNov. 2024', 'Vertical Voyager—'])
      expect(screen.getByText('Dec. 2024')).toHaveAttribute('dateTime', '2024-12-04T14:16:34.000Z')
    })

    it('opens on the first badge and shows the picked one in place of the avatar', async () => {
      await open()
      expect(screen.getByRole('heading', { name: 'Festive Trail' })).toBeInTheDocument()
      expect(screen.getByText('Unlocked: Dec. 2024')).toBeInTheDocument()
      expect(screen.getByText('Followed the trail')).toBeInTheDocument()
      await userEvent.click(screen.getByRole('button', { name: /^Vertical Voyager/ }))
      expect(screen.getByRole('heading', { name: 'Vertical Voyager' })).toBeInTheDocument()
      expect(screen.getByText('Locked')).toBeInTheDocument()
    })

    it("a tiered badge opens on its latest tier; someone else's passport lists only reached tiers", async () => {
      await open()
      await userEvent.click(screen.getByRole('button', { name: /^Traveler/ }))
      expect(screen.getByRole('heading', { name: 'Traveler Starter' })).toBeInTheDocument()
      expect(screen.getByText('1 scene explored')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Traveler Starter' })).toBeInTheDocument()
      expect(screen.queryByRole('button', { name: 'Traveler Bronze' })).not.toBeInTheDocument()
      expect(screen.queryByText(/NEXT TIER/)).not.toBeInTheDocument()
      expect(screen.queryByRole('progressbar')).not.toBeInTheDocument()
    })

    it('your own passport shows every tier, the next one to reach, and the progress toward it', async () => {
      await open(true)
      await userEvent.click(screen.getByRole('button', { name: /^Traveler/ }))
      await userEvent.click(screen.getByRole('button', { name: 'Traveler Bronze' }))
      expect(screen.getByRole('heading', { name: 'Traveler Bronze' })).toBeInTheDocument()
      expect(screen.getByText('Locked')).toBeInTheDocument()
      expect(screen.getByText('50 scenes explored', { selector: 'p' })).toBeInTheDocument()
      expect(screen.getByText('NEXT TIER: Bronze')).toBeInTheDocument()
      expect(screen.getAllByText('5/50')).toHaveLength(2) // the card's bar and the detail's
    })
  })

  it('Photos tab renders the camera-reel photos', async () => {
    render(<ProfilePassport profile={{ ...profile, photos: ['https://x/p1.png', 'https://x/p2.png'] }} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('tab', { name: 'PHOTOS' }))
    expect(screen.getAllByRole('link').some((a) => a.getAttribute('href') === 'https://x/p1.png')).toBe(true)
  })

  it('shows a graceful empty state when the user has no details', () => {
    render(<ProfilePassport profile={{ address: '0xnobody', name: 'Nobody', hasClaimedName: false, isGuest: false }} onClose={vi.fn()} />)
    expect(screen.getByText(/no details to show/i)).toBeInTheDocument()
  })

  it('switches tabs (Photos shows empty state)', async () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('tab', { name: 'PHOTOS' }))
    expect(screen.getByText(/No photos shared yet/i)).toBeInTheDocument()
  })
})

// Equipped Wearables / Equipped Emotes — the passport's read-only item grid (unity-explorer's
// EquippedItems module). The rules pinned here: what earns a tile, what earns a SHOP link, and the
// section order the Overview tab is supposed to follow.
const SHOP_TIARA = 'https://decentraland.org/shop/item/0xc0ffee/3'
const SHOP_DISCO = 'https://decentraland.org/shop/item/0xdecade/1'

const TIARA: Wearable = {
  urn: 'urn:decentraland:matic:collections-v2:0xc0ffee:3',
  name: 'Neon Tiara', rarity: 'legendary', category: 'tiara', equipped: true, shopUrl: SHOP_TIARA
}
const BASE_HAIR: Wearable = {
  urn: 'urn:decentraland:off-chain:base-avatars:casual_hair_01',
  name: 'Casual Hair', rarity: 'base', category: 'hair', equipped: true // off-chain: no listing
}
const BODY_SHAPE: Wearable = {
  urn: 'urn:decentraland:off-chain:base-avatars:BaseFemale',
  name: 'Base Female', rarity: 'base', category: 'body_shape', equipped: true
}
const DISCO: Emote = { slot: 0, urn: 'urn:decentraland:matic:collections-v2:0xdecade:1', name: 'Disco', rarity: 'epic', shopUrl: SHOP_DISCO }
const WAVE: Emote = { slot: 1, urn: 'urn:decentraland:off-chain:base-emotes:wave', name: 'Wave', rarity: 'base' }

const equippedProfile: Profile = {
  ...profile,
  links: undefined,
  equippedWearables: [TIARA, BASE_HAIR, BODY_SHAPE],
  equippedEmotes: [DISCO, WAVE]
}

describe('passport equipped items', () => {
  it('renders both sections with a tile per item', () => {
    render(<ProfilePassport profile={equippedProfile} onClose={vi.fn()} />)
    expect(screen.getByRole('heading', { name: 'Equipped Wearables' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Equipped Emotes' })).toBeInTheDocument()
    for (const name of ['Neon Tiara', 'Casual Hair', 'Disco', 'Wave']) {
      expect(screen.getByText(name)).toBeInTheDocument()
    }
  })

  it('shows a SHOP link only for on-chain collectibles', () => {
    render(<ProfilePassport profile={equippedProfile} onClose={vi.fn()} />)
    // One per collectible (the tiara + the Disco emote); the base hair and base emote get none.
    const shop = screen.getAllByRole('link', { name: 'Shop' })
    expect(shop.map((a) => a.getAttribute('href')).sort()).toEqual([SHOP_TIARA, SHOP_DISCO].sort())
    for (const a of shop) {
      expect(a).toHaveAttribute('target', '_blank')
      expect(a).toHaveAttribute('rel', 'noopener')
    }
  })

  it('drops the body shape (Unity skips BODY_SHAPE before filling the grid)', () => {
    render(<ProfilePassport profile={equippedProfile} onClose={vi.fn()} />)
    expect(screen.queryByText('Base Female')).toBeNull()
  })

  // The same emote can sit in two wheel slots (the emote list isn't deduped, unlike the wearable
  // set). Both tiles render either way on the first pass, so the thing to pin is that they carry
  // DISTINCT keys — with a duplicate key React's reconciliation of this list is undefined.
  it('gives the same emote in two wheel slots distinct keys', () => {
    const warn = vi.spyOn(console, 'error').mockImplementation(() => {})
    const twice: Emote[] = [{ ...WAVE, slot: 1 }, { ...WAVE, slot: 4 }]
    render(<ProfilePassport profile={{ ...equippedProfile, equippedEmotes: twice }} onClose={vi.fn()} />)
    expect(screen.getAllByText('Wave')).toHaveLength(2)
    expect(warn.mock.calls.flat().join(' ')).not.toMatch(/same key/i)
    warn.mockRestore()
  })

  it('orders the Overview as About Me → Equipped → Badges', () => {
    render(<ProfilePassport profile={equippedProfile} onClose={vi.fn()} />)
    const titles = screen.getAllByRole('heading', { level: 2 }).map((h) => h.textContent)
    expect(titles).toEqual(['About Me', 'Equipped Wearables', 'Equipped Emotes', 'Badges'])
  })

  it('omits a section when nothing is equipped in it', () => {
    render(<ProfilePassport profile={{ ...equippedProfile, equippedEmotes: [] }} onClose={vi.fn()} />)
    expect(screen.getByRole('heading', { name: 'Equipped Wearables' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Equipped Emotes' })).toBeNull()
  })

  it('counts as Overview content on its own (no empty state)', () => {
    const onlyEquipped: Profile = {
      address: '0xnobody', name: 'Nobody', hasClaimedName: false, isGuest: false,
      equippedWearables: [TIARA]
    }
    render(<ProfilePassport profile={onlyEquipped} onClose={vi.fn()} />)
    expect(screen.queryByText(/no details to show/i)).toBeNull()
    expect(screen.getByText('Neon Tiara')).toBeInTheDocument()
  })

  it('a body-shape-only equipped set leaves no section behind', () => {
    render(<ProfilePassport profile={{ ...equippedProfile, equippedWearables: [BODY_SHAPE] }} onClose={vi.fn()} />)
    expect(screen.queryByRole('heading', { name: 'Equipped Wearables' })).toBeNull()
  })
})
