// Read every page of a catalyst /explorer listing ({ elements, totalAmount }). Pure (no SDK
// imports) so react-web's vitest can cover it. A failed page rejects the whole read, so callers
// never cache a partial or empty list as if it were the player's collection.

export type Page<T> = { elements?: T[]; totalAmount?: number }

export async function readAllPages<T>(
  load: (pageNum: number) => Promise<Page<T>>,
  opts: { pageSize: number; maxPages: number }
): Promise<T[]> {
  const out: T[] = []
  for (let pageNum = 1; pageNum <= opts.maxPages; pageNum++) {
    const page = await load(pageNum)
    const elements = page.elements ?? []
    out.push(...elements)
    const total = page.totalAmount ?? out.length
    if (elements.length < opts.pageSize || out.length >= total) break
  }
  return out
}
