// Which equipped categories an avatar hides, with the engine's rules (collectibles/src/wearables.rs,
// avatar/src/lib.rs). Shared with the bridge scene; pure, so vitest covers it.

export interface HideData {
  hides?: string[]
  replaces?: string[]
  removesDefaultHiding?: string[]
  representations?: Array<{ bodyShapes?: string[]; overrideHides?: string[]; overrideReplaces?: string[] }>
}

const SKIN_HIDES = ['head', 'hair', 'facial_hair', 'mouth', 'eyebrows', 'eyes', 'upper_body', 'lower_body', 'feet', 'hands_wear', 'body_shape']

// Earlier categories win: a hidden item's own hides don't apply.
const HIDES_ORDER = [
  'skin', 'upper_body', 'hands_wear', 'lower_body', 'feet', 'helmet', 'hat', 'top_head', 'mask', 'eyewear',
  'earring', 'tiara', 'hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair', 'body_shape'
]

const same = (a: string, b: string): boolean => a.toLowerCase() === b.toLowerCase()

/** The categories an item hides when worn on `bodyShape`. */
export function itemHides(category: string, data: HideData | undefined, bodyShape: string | undefined): string[] {
  const rep = data?.representations?.find((r) => bodyShape != null && (r.bodyShapes ?? []).some((b) => same(b, bodyShape)))
  const hides = new Set(rep?.overrideHides?.length ? rep.overrideHides : (data?.hides ?? []))
  for (const c of rep?.overrideReplaces?.length ? rep.overrideReplaces : (data?.replaces ?? [])) hides.add(c)
  if (category === 'skin') for (const c of SKIN_HIDES) hides.add(c)
  if ((category === 'upper_body' || hides.has('upper_body')) && !(data?.removesDefaultHiding ?? []).includes('hands')) hides.add('hands')
  hides.delete(category)
  return [...hides]
}

/** Each hidden category → the category of the item hiding it. Force-rendered categories never hide. */
export function hiddenBy(equipped: Array<{ category: string; hides?: string[] }>, forceRender: string[]): Map<string, string> {
  const byCategory = new Map(equipped.map((w) => [w.category, w.hides ?? []]))
  const forced = new Set(forceRender)
  const pass = (skip?: Map<string, string>): Map<string, string> => {
    const out = new Map<string, string>()
    for (const cat of HIDES_ORDER) {
      if ((skip ?? out).has(cat)) continue
      for (const h of byCategory.get(cat) ?? []) if (!forced.has(h) && !out.has(h)) out.set(h, cat)
    }
    return out
  }
  return pass(pass())
}
