import { describe, expect, it } from 'vitest'
import { personalSign } from '../shell/sign'

describe('personalSign', () => {
  // web3.js docs: web3.eth.accounts.sign('Some data', key)
  it('matches a known EIP-191 signature', () => {
    expect(personalSign('0x4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318', 'Some data')).toBe(
      '0xb91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c'
    )
  })
})
