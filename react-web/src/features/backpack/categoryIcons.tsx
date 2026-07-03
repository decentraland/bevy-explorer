// Body-part category glyphs for the Backpack category column — the actual Unity
// (unity-explorer) backpack icons in src/assets/category-icons, statically imported (hashed,
// base-aware, a deleted/renamed file fails the BUILD). They're white-on-transparent PNGs
// rendered as a CSS mask so they take the tile's `color` (currentColor): white by default,
// accent on hover, white when selected. Unknown categories fall back to All — categories are
// runtime data from the engine, so that fallback stays.
import all from '../../assets/category-icons/all.png'
import body_shape from '../../assets/category-icons/body_shape.png'
import earring from '../../assets/category-icons/earring.png'
import eyebrows from '../../assets/category-icons/eyebrows.png'
import eyes from '../../assets/category-icons/eyes.png'
import eyewear from '../../assets/category-icons/eyewear.png'
import facial_hair from '../../assets/category-icons/facial_hair.png'
import feet from '../../assets/category-icons/feet.png'
import hair from '../../assets/category-icons/hair.png'
import hands_wear from '../../assets/category-icons/hands_wear.png'
import hat from '../../assets/category-icons/hat.png'
import helmet from '../../assets/category-icons/helmet.png'
import lower_body from '../../assets/category-icons/lower_body.png'
import mask from '../../assets/category-icons/mask.png'
import mouth from '../../assets/category-icons/mouth.png'
import skin from '../../assets/category-icons/skin.png'
import tiara from '../../assets/category-icons/tiara.png'
import top_head from '../../assets/category-icons/top_head.png'
import upper_body from '../../assets/category-icons/upper_body.png'

const ART: Record<string, string> = {
  all, body_shape, earring, eyebrows, eyes, eyewear, facial_hair, feet, hair, hands_wear,
  hat, helmet, lower_body, mask, mouth, skin, tiara, top_head, upper_body
}

export function CategoryIcon({ category, size = 28 }: { category: string; size?: number }): React.JSX.Element {
  const url = `url(${ART[category] ?? all})`
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
