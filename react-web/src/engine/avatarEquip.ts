// The avatar look the Backpack edits, and the setAvatar deploy built from it. Shared with the bridge
// scene (like protocol.ts); pure, so react-web's vitest covers it.

import type { Color3, SetAvatarData } from './generated'

export interface AvatarLook {
  bodyShape: string
  eyes: Color3 | null
  hair: Color3 | null
  skin: Color3 | null
  wearables: string[]
  /** The emote wheel's slots as the profile has them ('' = empty). */
  emotes: string[]
  forceRender: string[]
}

/** The deploy that turns `from` into `look`. The body and colors only go when they changed (a base
 *  also rewrites the name); force-render always goes, since sending [] erases the overrides the
 *  player set in another client. */
export function lookDeploy(look: AvatarLook, from: AvatarLook, name: string): SetAvatarData {
  const baseChanged =
    look.bodyShape !== from.bodyShape ||
    JSON.stringify([look.eyes, look.hair, look.skin]) !== JSON.stringify([from.eyes, from.hair, from.skin])
  return {
    ...(baseChanged && {
      base: { name, bodyShapeUrn: look.bodyShape, eyesColor: look.eyes, hairColor: look.hair, skinColor: look.skin }
    }),
    equip: { wearableUrns: look.wearables, emoteUrns: look.emotes, forceRender: look.forceRender }
  }
}

export function sameLook(a: AvatarLook, b: AvatarLook): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}
