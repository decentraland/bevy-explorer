// Body shapes in the backpack. Shared with the bridge scene (like protocol.ts); pure, so vitest covers it.

// The two avatar base body shapes; they are the whole body_shape category.
const BODY_SHAPE_URN = /:base-avatars:base(male|female)$/i

/** Body shapes an item has a representation for (undefined when the catalog didn't say). */
export function bodyShapesOf(reps: Array<{ bodyShapes?: string[] }> | undefined): string[] | undefined {
  if (reps == null) return undefined
  return [...new Set(reps.flatMap((r) => r.bodyShapes ?? []))]
}

/** A body shape is the avatar's base, not a wearable: the engine skips it in wearableUrns. */
export function splitBodyShape(urns: string[]): { bodyShape: string | undefined; wearables: string[] } {
  return { bodyShape: urns.find((u) => BODY_SHAPE_URN.test(u)), wearables: urns.filter((u) => !BODY_SHAPE_URN.test(u)) }
}

// Whether an item can render on the current body shape (Unity BackpackItemView.IsCompatibleWithBodyShape).
export function isCompatible(item: { category: string; bodyShapes?: string[] }, bodyShape: string | undefined): boolean {
  if (bodyShape == null || item.category === 'body_shape' || item.bodyShapes == null) return true
  const want = bodyShape.toLowerCase()
  return item.bodyShapes.some((b) => b.toLowerCase() === want)
}
