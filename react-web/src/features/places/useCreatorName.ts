import { isAddress, shortAddr } from '../../lib/identity'
import { useProfile } from '../session/profileStore'

/** A place's creator for display: a bare owner address becomes that wallet's profile name. */
export function useCreatorName(creator: string | null): string | null {
  const address = creator != null && isAddress(creator) ? creator : ''
  const profile = useProfile(address)
  if (address === '') return creator
  return profile?.name || shortAddr(address)
}
