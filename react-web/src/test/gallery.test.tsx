import { describe, it, expect } from 'vitest'
import { act } from '@testing-library/react'
import type { GalleryPhoto } from '../engine/protocol'
import { renderSession, enterAsGuest } from './harness'

// DOMAIN: gallery (camera reel) — fetch the reel, lazy per-photo metadata, delete.
describe('gallery domain', () => {
  const photo = (id: string, dateTime: string): GalleryPhoto => ({
    id,
    url: `https://img/${id}.jpg`,
    thumbnailUrl: `https://img/${id}_t.jpg`,
    dateTime
  })

  it('opening the gallery fetches it once (cached per session)', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().gallery.toggle())
    act(() => h.session().gallery.toggle()) // close
    act(() => h.session().gallery.toggle()) // reopen — must NOT refetch
    expect(h.driver.sentOf('getGallery')).toHaveLength(1)
  })

  it('gallery stream sorts newest-first and sets storage + loaded', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    expect(h.session().gallery.loaded).toBe(false)
    h.driver.emit({
      kind: 'gallery',
      photos: [photo('old', '1000'), photo('new', '3000'), photo('mid', '2000')],
      current: 3,
      max: 500
    })
    expect(h.session().gallery.loaded).toBe(true)
    expect(h.session().gallery.list.map((p) => p.id)).toEqual(['new', 'mid', 'old'])
    expect(h.session().gallery.current).toBe(3)
    expect(h.session().gallery.max).toBe(500)
  })

  it('loadPhoto requests metadata and the stream fills the cache', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    act(() => h.session().gallery.loadPhoto('g1'))
    expect(h.driver.last('getGalleryPhoto')).toEqual({ kind: 'getGalleryPhoto', id: 'g1' })
    h.driver.emit({ kind: 'galleryPhoto', id: 'g1', meta: { sceneName: 'Genesis Plaza', x: -9, y: 14 } })
    expect(h.session().gallery.metas['g1']?.sceneName).toBe('Genesis Plaza')
  })

  it('remove posts a delete and optimistically drops the photo', async () => {
    const h = renderSession()
    await enterAsGuest(h)
    h.driver.emit({ kind: 'gallery', photos: [photo('a', '2000'), photo('b', '1000')], current: 2, max: 500 })
    act(() => h.session().gallery.remove('a'))
    expect(h.driver.last('deleteGalleryPhoto')).toEqual({ kind: 'deleteGalleryPhoto', id: 'a' })
    expect(h.session().gallery.list.map((p) => p.id)).toEqual(['b'])
    expect(h.session().gallery.current).toBe(1)
  })
})
