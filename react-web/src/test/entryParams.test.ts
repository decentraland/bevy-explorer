// DOMAIN: the entry url's accepted-parameter set (lib/entryParams.ts) and the dialog that names
// what a link carried that nothing reads (features/gate/EntryParamsDialog.tsx).

import { describe, expect, it } from 'vitest'
import { WEB_PARAMS } from '../engine/generated'
import { acceptedEntryParams, unrecognisedEntryParams } from '../lib/entryParams'

describe('acceptedEntryParams', () => {
  it('is every link-settable engine param plus the page switches, with a doc each', () => {
    const accepted = acceptedEntryParams()
    const names = accepted.map((p) => p.name)
    for (const p of WEB_PARAMS) expect(names.includes(p.name)).toBe(p.delivery !== 'host')
    for (const p of accepted) expect(p.doc.length).toBeGreaterThan(0)
    expect(new Set(names).size).toBe(names.length)
    expect(names).toContain('guest')
    expect(names).toContain('fps')
    // host-forced and debug switches are known, not advertised
    for (const internal of ['native', 'hud', 'mock', 'gate', 'simerror']) expect(names).not.toContain(internal)
  })
})

describe('unrecognisedEntryParams', () => {
  it('is empty for a link made of accepted params only', () => {
    expect(unrecognisedEntryParams(new URLSearchParams('?realm=x&position=1,2&mock=1&preview'))).toEqual([])
    expect(unrecognisedEntryParams(new URLSearchParams('?native=1&hud=0&gate=gpu&simerror=realm'))).toEqual([])
  })

  it('names each unknown key once, including the host-only editor flag', () => {
    const q = new URLSearchParams('?realm=x&reaml=y&editor&reaml=z&initialRealm=w')
    expect(unrecognisedEntryParams(q)).toEqual(['reaml', 'editor', 'initialRealm'])
  })
})
