// Body shapes in the backpack. Shared with the bridge scene (like protocol.ts); pure, so vitest covers it.

type WithRepresentations = { entity?: { metadata?: { data?: { representations?: Array<{ bodyShapes?: string[] }> } } } }

/** Body shapes an item has a representation for (undefined when the catalog didn't say). */
export function bodyShapesOf(el: WithRepresentations): string[] | undefined {
  const reps = el.entity?.metadata?.data?.representations
  if (reps == null) return undefined
  return [...new Set(reps.flatMap((r) => r.bodyShapes ?? []))]
}
