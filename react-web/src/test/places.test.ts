import { describe, it, expect } from 'vitest'
import {
  placeCoords,
  placeCreator,
  placeIsFeatured,
  placePlayers,
  placeRating,
  placeTeleport,
  type DiscoverPlace
} from '../features/places/placesApi'

// A genesis-city parcel place (base_position set, votes present).
const parcelPlace: DiscoverPlace = {
  id: 'p1',
  title: 'Genesis Plaza',
  description: 'The heart of Decentraland',
  image: 'https://img/genesis.png',
  positions: ['-3,-2', '-3,-1'],
  base_position: '-3,-2',
  owner: '0xabc',
  contact_name: 'DCL Foundation',
  categories: ['poi', 'social'],
  likes: 90,
  dislikes: 10,
  user_count: 142,
  user_name: 'DCL'
}

// A world (off-atlas realm).
const worldPlace: DiscoverPlace = {
  id: 'w1',
  title: 'Bloom Garden',
  description: 'A world',
  image: '',
  positions: ['0,0'],
  owner: null,
  world: true,
  world_name: 'limmagarden.dcl.eth'
}

describe('placeCoords', () => {
  it('returns the base_position for a parcel place', () => {
    expect(placeCoords(parcelPlace)).toBe('-3,-2')
  })
  it('falls back to the first position when base_position is absent', () => {
    expect(placeCoords({ ...parcelPlace, base_position: undefined })).toBe('-3,-2')
  })
  it('returns the world_name for a world', () => {
    expect(placeCoords(worldPlace)).toBe('limmagarden.dcl.eth')
  })
})

describe('placeRating', () => {
  it('rounds likes/(likes+dislikes) to a percentage', () => {
    expect(placeRating(parcelPlace)).toBe(90)
  })
  it('is 0 when there are no votes', () => {
    expect(placeRating({ ...parcelPlace, likes: 0, dislikes: 0 })).toBe(0)
  })
  it('is 0 when vote data is missing (worlds)', () => {
    expect(placeRating(worldPlace)).toBe(0)
  })
})

describe('placePlayers', () => {
  it('returns user_count', () => {
    expect(placePlayers(parcelPlace)).toBe(142)
  })
  it('defaults to 0 when absent', () => {
    expect(placePlayers(worldPlace)).toBe(0)
  })
})

describe('placeCreator', () => {
  it('prefers user_name, then contact_name, then owner', () => {
    expect(placeCreator(parcelPlace)).toBe('DCL')
    expect(placeCreator({ ...parcelPlace, user_name: undefined })).toBe('DCL Foundation')
    expect(placeCreator({ ...parcelPlace, user_name: undefined, contact_name: undefined })).toBe('0xabc')
  })
  it('returns empty string when nothing is set', () => {
    expect(placeCreator(worldPlace)).toBe('')
  })
})

describe('placeIsFeatured', () => {
  it('is true when categories include poi', () => {
    expect(placeIsFeatured(parcelPlace)).toBe(true)
  })
  it('is false otherwise', () => {
    expect(placeIsFeatured(worldPlace)).toBe(false)
    expect(placeIsFeatured({ ...parcelPlace, categories: ['social'] })).toBe(false)
  })
})

describe('placeTeleport', () => {
  it('returns a parcel target parsed from base_position', () => {
    expect(placeTeleport(parcelPlace)).toEqual({ kind: 'parcel', x: -3, y: -2 })
  })
  it('returns a world target for a world', () => {
    expect(placeTeleport(worldPlace)).toEqual({ kind: 'world', realm: 'limmagarden.dcl.eth' })
  })
  it('returns null for a world without a name', () => {
    expect(placeTeleport({ ...worldPlace, world_name: undefined })).toBeNull()
  })
  it('returns null when no coordinate is parseable', () => {
    expect(placeTeleport({ ...parcelPlace, base_position: 'not-a-coord', positions: [] })).toBeNull()
    expect(placeTeleport({ ...parcelPlace, base_position: undefined, positions: [] })).toBeNull()
  })
})
