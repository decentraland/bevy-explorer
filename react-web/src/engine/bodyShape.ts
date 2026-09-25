// Body shapes in the backpack. Shared with the bridge scene (like protocol.ts); pure, so vitest covers it.

// The two avatar base body shapes; they are the whole body_shape category.
const BODY_SHAPE_URN = /:base-avatars:base(male|female)$/i

type WithRepresentations = { entity?: { metadata?: { data?: { representations?: Array<{ bodyShapes?: string[] }> } } } }

/** Body shapes an item has a representation for (undefined when the catalog didn't say). */
export function bodyShapesOf(el: WithRepresentations): string[] | undefined {
  const reps = el.entity?.metadata?.data?.representations
  if (reps == null) return undefined
  return [...new Set(reps.flatMap((r) => r.bodyShapes ?? []))]
}

/** A body shape is the avatar's base, not a wearable: the engine skips it in wearableUrns. */
export function splitBodyShape(urns: string[]): { bodyShape: string | undefined; wearables: string[] } {
  return { bodyShape: urns.find((u) => BODY_SHAPE_URN.test(u)), wearables: urns.filter((u) => !BODY_SHAPE_URN.test(u)) }
}
