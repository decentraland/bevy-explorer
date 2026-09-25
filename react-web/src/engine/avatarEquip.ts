// The equip half of a setAvatar deploy, rebuilt from the live player for whatever the caller doesn't
// change. Shared with the bridge scene (like protocol.ts); pure, so react-web's vitest covers it.

type EquipSource = { wearables?: readonly unknown[]; emotes?: readonly unknown[]; forceRender?: readonly unknown[] } | null | undefined

export function equipPayload(
  me: EquipSource,
  change: { wearableUrns?: string[]; emoteUrns?: string[] }
): { wearableUrns: string[]; emoteUrns: string[]; forceRender: string[] } {
  return {
    wearableUrns: change.wearableUrns ?? (me?.wearables ?? []).map(String),
    emoteUrns: change.emoteUrns ?? (me?.emotes ?? []).map(String),
    // Always carried over: sending [] erases force-render overrides the player set in another client.
    forceRender: (me?.forceRender ?? []).map(String)
  }
}
