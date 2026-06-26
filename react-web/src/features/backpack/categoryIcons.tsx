// Body-part category glyphs for the Backpack category column — the actual Unity
// (unity-explorer) backpack icons, copied to /public/category-icons. They're white-on-
// transparent PNGs rendered as a CSS mask so they take the tile's `color` (currentColor):
// white by default, accent on hover, white when selected. Unknown categories fall back to All.

const KNOWN = new Set([
  'body_shape', 'hair', 'eyebrows', 'eyes', 'mouth', 'facial_hair', 'upper_body',
  'hands_wear', 'lower_body', 'feet', 'hat', 'eyewear', 'mask', 'earring', 'helmet',
  'tiara', 'top_head', 'skin'
])

export function CategoryIcon({ category, size = 28 }: { category: string; size?: number }): React.JSX.Element {
  const name = KNOWN.has(category) ? category : 'all'
  const url = `url(/category-icons/${name}.png)`
  return (
    <span
      aria-hidden="true"
      style={{
        display: 'inline-block',
        width: size,
        height: size,
        backgroundColor: 'currentColor',
        maskImage: url,
        WebkitMaskImage: url,
        maskRepeat: 'no-repeat',
        WebkitMaskRepeat: 'no-repeat',
        maskPosition: 'center',
        WebkitMaskPosition: 'center',
        maskSize: 'contain',
        WebkitMaskSize: 'contain'
      }}
    />
  )
}
