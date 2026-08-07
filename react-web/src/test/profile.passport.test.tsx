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
  badges: [{ id: 'b1', name: 'Festive Trail' }],
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

  it('ADD FRIEND when not a friend; FRIEND (disabled) when already', async () => {
    const onAddFriend = vi.fn()
    const { rerender } = render(<ProfilePassport profile={profile} onAddFriend={onAddFriend} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'ADD FRIEND' }))
    expect(onAddFriend).toHaveBeenCalledWith(profile.address)

    rerender(<ProfilePassport profile={profile} relationship="friend" onAddFriend={onAddFriend} onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'FRIEND' })).toBeDisabled()
  })

  it('Add Friend requests, then flips to REQUESTED (optimistic feedback)', async () => {
    const onAddFriend = vi.fn()
    render(<ProfilePassport profile={profile} onAddFriend={onAddFriend} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'ADD FRIEND' }))
    expect(onAddFriend).toHaveBeenCalledWith(profile.address)
    expect(screen.getByRole('button', { name: 'REQUESTED' })).toBeDisabled()
  })

  it('shows REQUESTED (not Add Friend) when a request is already pending', () => {
    render(<ProfilePassport profile={profile} relationship="requested" onClose={vi.fn()} />)
    expect(screen.getByRole('button', { name: 'REQUESTED' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'ADD FRIEND' })).toBeNull()
  })

  it('hides the friend action for an incoming request (would duplicate-request otherwise)', () => {
    render(<ProfilePassport profile={profile} relationship="incoming" onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
  })

  it('hides the friend action for a blocked user', () => {
    render(<ProfilePassport profile={profile} relationship="blocked" onClose={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /FRIEND/i })).toBeNull()
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

  it('Photos tab renders the camera-reel photos', async () => {
    render(<ProfilePassport profile={{ ...profile, photos: ['https://x/p1.png', 'https://x/p2.png'] }} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'PHOTOS' }))
    expect(screen.getAllByRole('link').some((a) => a.getAttribute('href') === 'https://x/p1.png')).toBe(true)
  })

  it('shows a graceful empty state when the user has no details', () => {
    render(<ProfilePassport profile={{ address: '0xnobody', name: 'Nobody', hasClaimedName: false, isGuest: false }} onClose={vi.fn()} />)
    expect(screen.getByText(/no details to show/i)).toBeInTheDocument()
  })

  it('switches tabs (Photos shows empty state)', async () => {
    render(<ProfilePassport profile={profile} onClose={vi.fn()} />)
    await userEvent.click(screen.getByRole('button', { name: 'PHOTOS' }))
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
