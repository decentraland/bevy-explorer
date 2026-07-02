// A single emote-wheel slot card — a faithful rebuild of the Figma EmoteWheelSlot
// (node 10386-4701), parameterised by rarity. Structure mirrors the export exactly:
//   • #691FA9 purple base (constant)
//   • two large blurred glows whose colours/size/position/blur are rarity-specific
//   • the exact folded corner flap (rarity colour) with its shadow and the detailed icon
//   • the emote silhouette image (catalyst thumbnail), clipped to the card
//   • a light border
// `rotate` tilts the card for the wheel while counter-rotating the silhouette + number.

import { useId } from 'react'
import styles from './EmoteSlot.module.css'

const VB_W = 182
const VB_H = 168
// Silhouette box centre — counter-rotating the thumbnail around ITS OWN centre (not the
// card centre) keeps it upright while letting it follow the card to the outer edge.
const THUMB_CX = 92
const THUMB_CY = 74

// Per-rarity glows + flap, transcribed from the Figma EmoteWheelSlot variants. Figma ellipse
// coordinates are in the 174×160 card frame; mapped to this 182×168 viewBox with a +4,+4
// offset (cx = left + size/2 + 4). `b` is the gaussian stdDeviation (= the Figma blur px).
type Glow = { c: string; cx: number; cy: number; r: number; b: number; o?: number }
type RaritySpec = { flap: string; glows: [Glow, Glow] }

const RARITIES: Record<string, RaritySpec> = {
  base: { flap: '#A09BA8', glows: [
    { c: '#CFCDD4', cx: 156.62, cy: 134.02, r: 123.28, b: 53.79 },
    { c: '#FCFCFC', cx: 33.93, cy: 0.93, r: 121.93, b: 53.79, o: 0.8 }
  ] },
  common: { flap: '#73D3D3', glows: [
    { c: '#73D3D3', cx: 137, cy: 115, r: 119, b: 89.96 },
    { c: '#B5FFF6', cx: 44, cy: 32, r: 68, b: 50 }
  ] },
  uncommon: { flap: '#FF8362', glows: [
    { c: '#FF8362', cx: 137, cy: 115, r: 119, b: 81.65 },
    { c: '#FFE0B2', cx: 29.47, cy: 18.47, r: 77.47, b: 50 }
  ] },
  rare: { flap: '#34CE76', glows: [
    { c: '#34CE76', cx: 137, cy: 115, r: 106, b: 53.79 },
    { c: '#73FFAF', cx: 32.48, cy: 19.06, r: 77.47, b: 50 }
  ] },
  epic: { flap: '#438FFF', glows: [
    { c: '#5C9EFF', cx: 136, cy: 102, r: 102, b: 53.79 },
    { c: '#4DEAFF', cx: 12.47, cy: 21.47, r: 77.47, b: 50 }
  ] },
  legendary: { flap: '#A14BF3', glows: [
    { c: '#B66CFF', cx: 121, cy: 112, r: 83, b: 41.72 },
    { c: '#E579FB', cx: 12.47, cy: 21.47, r: 77.47, b: 50 }
  ] },
  mythic: { flap: '#FF4BED', glows: [
    { c: '#FF4BED', cx: 121, cy: 112, r: 83, b: 53.79 },
    { c: '#FFA6C1', cx: 12.47, cy: 21.47, r: 77.47, b: 53.79 }
  ] },
  exotic: { flap: '#9CD71E', glows: [
    { c: '#CAFF73', cx: 121, cy: 112, r: 116, b: 74.89 },
    { c: '#CAFF73', cx: 12.47, cy: 21.47, r: 77.47, b: 58.66 }
  ] },
  unique: { flap: '#FEA217', glows: [
    { c: '#FFDD2C', cx: 70, cy: 157, r: 91, b: 53.79 },
    { c: '#FFAB2C', cx: 132.9, cy: 39.9, r: 53.9, b: 53.79 }
  ] }
}

// Exact Figma paths (viewBox 182×168).
const BORDER_PATH =
  'M14.8164 12.709C64.4508 -0.703243 116.612 -1.5304 166.64 10.3027C176.559 12.6489 182.063 22.9775 179.27 32.6738L145.125 151.212C142.308 160.99 132.238 166.37 122.526 164.608C103.049 161.075 83.0735 161.392 63.7158 165.54C54.0636 167.609 43.8302 162.551 40.71 152.865L2.89062 35.4668C-0.202676 25.8636 4.97707 15.368 14.8164 12.709Z'
const FILL_PATH =
  'M4.79456 34.854C2.02502 26.2568 6.68143 16.9787 15.3385 14.6393C64.652 1.31383 116.475 0.492713 166.179 12.2493C174.905 14.3132 179.848 23.4393 177.348 32.12L143.203 150.658C140.703 159.339 131.704 164.24 122.883 162.64C103.149 159.061 82.9097 159.381 63.2967 163.584C54.53 165.463 45.3831 160.849 42.6136 152.252L4.79456 34.854Z'
const FLAP_PATH =
  'M47.9447 7.70117C43.9328 8.34457 40.4114 10.7359 38.638 14.3936L17.1322 58.75C15.343 62.4402 15.0466 66.6786 16.304 70.582L4.79428 34.8545C2.02478 26.2574 6.68134 16.9791 15.3382 14.6396C26.1034 11.7307 36.9885 9.41758 47.9447 7.70117Z'
const FLAP_SHADOW =
  'M17.1325 58.7501C15.3433 62.4403 15.047 66.6787 16.3044 70.5822L13.1836 60.8956L33.8238 24.3209L17.1325 58.7501Z'
const ICON_BODY =
  'M11.0036 21.9181C10.6762 22.1695 10.615 22.639 10.867 22.9659L15.3604 26.5831L17.129 28.6697L15.533 31.4881L15.1114 35.3803C15.162 35.8414 15.6155 36.1457 16.0614 36.0179C16.4192 35.9153 16.6472 35.5652 16.5963 35.1964L17.0108 31.9443L19.1989 28.861L20.0563 33.9719C20.1331 34.4298 20.5915 34.7189 21.0379 34.5909C21.4127 34.4834 21.65 34.1151 21.5929 33.7294L20.6933 27.6476L19.6397 25.3561L21.2475 20.6279C21.4369 20.0711 20.9223 19.5309 20.357 19.6931C20.1239 19.7599 19.9394 19.9382 19.8647 20.1689L19.0229 22.5681C18.7079 23.4659 17.9771 24.1553 17.0625 24.4176C16.1661 24.6746 15.2008 24.4903 14.4621 23.921L12.0623 22.0715C11.8154 21.7316 11.3368 21.6622 11.0036 21.9181Z'

export function EmoteSlot({
  thumbnail,
  rarity = 'base',
  number,
  width = 174,
  rotate = 0
}: {
  thumbnail?: string
  rarity?: string
  number?: number
  width?: number
  /** Radial rotation for the wheel — the card tilts, the silhouette/number stay upright. */
  rotate?: number
}): React.JSX.Element {
  const id = useId().replace(/:/g, '')
  const src = thumbnail
  const spec = RARITIES[rarity] ?? RARITIES.base
  const counter = rotate ? `rotate(${-rotate} ${THUMB_CX} ${THUMB_CY})` : undefined
  // Number scales with the card and counter-rotates to stay upright in the wheel.
  const numStyle: React.CSSProperties = {
    fontSize: Math.round(width * 0.19),
    transform: `translateX(-50%)${rotate ? ` rotate(${-rotate}deg)` : ''}`
  }
  return (
    <div className={styles.slot} style={{ width, height: (width * VB_H) / VB_W, transform: rotate ? `rotate(${rotate}deg)` : undefined }}>
      <svg className={styles.bg} viewBox={`0 0 ${VB_W} ${VB_H}`} preserveAspectRatio="none">
        <defs>
          {spec.glows.map((g, i) => (
            <filter key={i} id={`blur-${id}-${i}`} x="-120" y="-120" width="420" height="420" filterUnits="userSpaceOnUse" colorInterpolationFilters="sRGB">
              <feGaussianBlur stdDeviation={g.b} />
            </filter>
          ))}
          <clipPath id={`clip-${id}`}>
            <path d={FILL_PATH} />
          </clipPath>
        </defs>

        {/* Purple base + the two rarity glows (main colour, then lighter accent). */}
        <path d={FILL_PATH} fill="#691FA9" />
        <g clipPath={`url(#clip-${id})`}>
          {spec.glows.map((g, i) => (
            <circle key={i} cx={g.cx} cy={g.cy} r={g.r} fill={g.c} opacity={g.o ?? 1} filter={`url(#blur-${id}-${i})`} />
          ))}
          {/* Silhouette (counter-rotated to stay upright in the wheel). */}
          {src && (
            <g transform={counter}>
              {/* Exact Figma placement: 108×108 at (38,20), square thumbnail fills the box. */}
              <image href={src} x="37.9961" y="20" width="108" height="108" preserveAspectRatio="none" />
            </g>
          )}
          {/* Folded corner flap + shadow + emote icon. */}
          <path d={FLAP_PATH} fill={spec.flap} />
          <path d={FLAP_SHADOW} fill="black" fillOpacity="0.1" />
          <path d={ICON_BODY} fill="#FCFCFC" />
          <circle cx="16.2543" cy="21.5305" r="1.87704" transform="rotate(-16 16.2543 21.5305)" fill="#FCFCFC" />
        </g>

        <path d={BORDER_PATH} fill="none" stroke="#f1eff5" strokeWidth="4" />
      </svg>
      {number != null && <span className={styles.num} style={numStyle}>{number}</span>}
    </div>
  )
}
