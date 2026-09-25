// Whether an item can render on the current body shape (Unity BackpackItemView.IsCompatibleWithBodyShape).
export function isCompatible(item: { category: string; bodyShapes?: string[] }, bodyShape: string | undefined): boolean {
  if (bodyShape == null || item.category === 'body_shape' || item.bodyShapes == null) return true
  const want = bodyShape.toLowerCase()
  return item.bodyShapes.some((b) => b.toLowerCase() === want)
}
