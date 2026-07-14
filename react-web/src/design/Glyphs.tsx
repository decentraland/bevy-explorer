// Glyphs — inline-SVG icon set ported from eordano/dcl-react-ui (atoms/icons,
// SocialIcons, StarWalletIcon, DclLogomark, ManaMark). These are additive and
// separate from the mask-based nav set in icons.tsx (Icon / IconName).
//
// Most glyphs take `{ size?, className? }` and paint with `currentColor` so they
// inherit the surrounding text color. A few keep extra knobs from the source
// (ring, stroke, open, network, …), all typed. Token vars (--gold, --font-sans)
// come from tokens.css.

import styles from './Glyphs.module.css'

interface GlyphProps {
  size?: number
  className?: string
}

interface StrokeGlyphProps extends GlyphProps {
  strokeWidth?: number
}

export function Coin({
  size = 18,
  ring = true,
  className,
}: GlyphProps & { ring?: boolean }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r={ring ? 10 : 11} fill="var(--gold)" stroke="#e0a429" strokeWidth="1.5" />
      {ring && <circle cx="12" cy="12" r="6.5" fill="none" stroke="#e0a429" strokeWidth="1.2" opacity=".6" />}
      <text x="12" y="12" textAnchor="middle" dominantBaseline="central" fontFamily="var(--font-sans, system-ui, sans-serif)" fontSize={ring ? 9 : 10} fontWeight="800" fill="#8a5a00">M</text>
    </svg>
  )
}

export function PersonIcon({ size = 11, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 12 12" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="6" cy="3.6" r="2.2" fill="currentColor" />
      <path d="M1.6 10.5c0-2.4 2-3.8 4.4-3.8s4.4 1.4 4.4 3.8z" fill="currentColor" />
    </svg>
  )
}

export function ManaIcon({ size = 18, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r="11" fill="#ff2d55" />
      <path d="M6 16l3.2-7 2.8 5 1.5-3 1.3 5z" fill="#fff" opacity=".92" />
    </svg>
  )
}

export function Mute({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M4 9v6h4l5 4V5L8 9H4z" fill="currentColor" />
      <path d="M16 9a3.5 3.5 0 0 1 0 6M18.5 7a6.5 6.5 0 0 1 0 10" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  )
}

export function Check({
  size = 16,
  className,
  stroke = '#fff',
  strokeWidth = 2.2,
}: GlyphProps & { stroke?: string; strokeWidth?: number }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 12.5l3 3 7-7" fill="none" stroke={stroke} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function Bag({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 7V6a5 5 0 0 1 10 0v1h2.2l.8 12.5a2 2 0 0 1-2 2.1H6a2 2 0 0 1-2-2.1L4.8 7H7Zm2 0h6V6a3 3 0 0 0-6 0v1Z" fill="currentColor" />
    </svg>
  )
}

export function Pin({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Zm0 9.5A2.5 2.5 0 1 1 12 6.5a2.5 2.5 0 0 1 0 5Z" fill="currentColor" />
    </svg>
  )
}

export function CameraIcon({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M9 4 7.5 6H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-2.5L15 4H9Zm3 5a4.2 4.2 0 1 1 0 8.4A4.2 4.2 0 0 1 12 9Zm0 2a2.2 2.2 0 1 0 0 4.4A2.2 2.2 0 0 0 12 11Z" fill="currentColor" />
    </svg>
  )
}

// Walking-person glyph — the "too far, get closer" hover state in bevy-ui-scene used a walking-
// figure sprite (avatar/hover-actions' unreachable icon); this is that concept re-drawn as a vector.
export function WalkIcon({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12.5" cy="4.5" r="2" fill="currentColor" />
      <path
        d="M9 8.6 12.6 7l3 1.7v3.5h-1.8V9.8l-1-.5v2.8l2.3 2.2-.7 6.6-1.8-.2.6-5.7-1.9-1.8-1.2 5.8-1.7-.4 1.5-7.1-1.6-.9v3.2H7.7V9.6L9 8.6Z"
        fill="currentColor"
      />
    </svg>
  )
}

export function People({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M8.5 11a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm7 0a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm-7 1.5C5.5 12.5 2 14 2 16.5V19h8v-2.5c0-1 .5-2 1.4-2.7a7 7 0 0 0-2.9-.8Zm7 0c-.6 0-1.2.1-1.8.2 1 .8 1.8 1.8 1.8 3.3V19h6v-2.5c0-2.5-3.5-4-6-4Z" fill="currentColor" />
    </svg>
  )
}

export function Help({ size = 16, className }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 20 20" width={size} height={size} aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" className={className}>
      <circle cx="10" cy="10" r="7" />
      <path d="M8 8a2 2 0 1 1 3 1.7c-.7.5-1 .9-1 1.8" />
      <circle cx="10" cy="14" r=".5" fill="currentColor" stroke="none" />
    </svg>
  )
}

export function ChevronDown({ size = 20, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M6 9l6 6 6-6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ChevronUp({ size = 20, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M18 15l-6-6-6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ChevronLeft({ size = 18, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M15 6l-6 6 6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ChevronRight({ size = 18, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M9 6l6 6-6 6" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function ChevronDownAlt({ size = 18, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M7 10l5 5 5-5" stroke="currentColor" strokeWidth={strokeWidth} fill="none" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function Heart({ size = 13, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M12 2 3 12l9 10 9-10L12 2Z" fill="currentColor" />
    </svg>
  )
}

export function Close({ size = 20, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" strokeWidth={strokeWidth} fill="none" strokeLinecap="round" />
    </svg>
  )
}

export function Caret({
  size = 13,
  className = '',
  open = false,
  strokeWidth = 1.7,
}: StrokeGlyphProps & { open?: boolean }): React.JSX.Element {
  return (
    <svg
      className={className + (open ? ' is-open' : '')}
      viewBox="0 0 16 16"
      width={size}
      height={size}
      aria-hidden="true"
    >
      <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" fill="none" />
    </svg>
  )
}

export function Section({
  size = 14,
  className,
  open = false,
}: GlyphProps & { open?: boolean }): React.JSX.Element {
  return (
    <svg viewBox="0 0 512 512" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      {open ? (
        <path d="M98.9 184.7l1.8 2.1 136 156.5c4.6 5.3 11.5 8.6 19.2 8.6 7.7 0 14.6-3.4 19.2-8.6L411 187.1l2.3-2.6c1.7-2.5 2.7-5.5 2.7-8.7 0-8.7-7.4-15.8-16.6-15.8H112.6c-9.2 0-16.6 7.1-16.6 15.8 0 3.3 1.1 6.4 2.9 8.9z" />
      ) : (
        <path d="M190.5 66.9l22.2-22.2c9.4-9.4 24.6-9.4 33.9 0L441 239c9.4 9.4 9.4 24.6 0 33.9L246.6 467.3c-9.4 9.4-24.6 9.4-33.9 0l-22.2-22.2c-9.5-9.5-9.3-25 .4-34.3L311.4 296H24c-13.3 0-24-10.7-24-24v-32c0-13.3 10.7-24 24-24h287.4L168.9 101.2c-9.8-9.3-10-24.8-.4-34.3z" />
      )}
    </svg>
  )
}

export function Search({
  size = 18,
  className,
  strokeWidth = 2,
  handle = 'M21 21l-4.3-4.3',
}: StrokeGlyphProps & { handle?: string }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth={strokeWidth} />
      <path d={handle} stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  )
}

export function Plus({ size = 18, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  )
}

export function Trash({ size = 18, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
    </svg>
  )
}

export function Pencil({ size = 18, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a1 1 0 0 0 0-1.41l-2.34-2.34a1 1 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z" />
    </svg>
  )
}

export function Gear({ size = 18, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M19.14 12.94a7.49 7.49 0 0 0 .05-.94 7.49 7.49 0 0 0-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.62l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7 7 0 0 0-1.62-.94l-.36-2.54a.49.49 0 0 0-.49-.42h-3.84a.49.49 0 0 0-.49.42l-.36 2.54a7 7 0 0 0-1.62.94l-2.39-.96a.5.5 0 0 0-.6.22L2.74 8.86a.5.5 0 0 0 .12.62l2.03 1.58a7.49 7.49 0 0 0 0 1.88l-2.03 1.58a.5.5 0 0 0-.12.62l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96a7 7 0 0 0 1.62.94l.36 2.54a.49.49 0 0 0 .49.42h3.84a.49.49 0 0 0 .49-.42l.36-2.54a7 7 0 0 0 1.62-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.62l-2.03-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z" />
    </svg>
  )
}

export function Copy({ size = 16, className, strokeWidth = 1.6 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <rect x="9" y="9" width="11" height="11" rx="2" stroke="currentColor" strokeWidth={strokeWidth} />
      <path d="M5 15V5a2 2 0 0 1 2-2h8" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  )
}

export function Info({ size = 18, className, strokeWidth = 1.7 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true" className={className}>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth={strokeWidth} fill="none" />
      <path d="M12 11v5M12 7.6v.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  )
}

export function CheckFill({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M9 16.17 4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z" />
    </svg>
  )
}

export function CheckBold({
  size = 18,
  className,
  strokeWidth = 2,
  compact = false,
}: StrokeGlyphProps & { compact?: boolean }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d={compact ? 'M3 8.5l3 3 7-7' : 'M20 6 9 17l-5-5'} stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

export function TriangleDown({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M7 10l5 5 5-5z" />
    </svg>
  )
}

export function Kebab({
  size = 18,
  className,
  r = 2,
}: GlyphProps & { r?: number }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <circle cx="5" cy="12" r={r} />
      <circle cx="12" cy="12" r={r} />
      <circle cx="19" cy="12" r={r} />
    </svg>
  )
}

export function GridView({ size = 16, className }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <rect x="3" y="3" width="7" height="7" rx="1.5" />
      <rect x="14" y="3" width="7" height="7" rx="1.5" />
      <rect x="3" y="14" width="7" height="7" rx="1.5" />
      <rect x="14" y="14" width="7" height="7" rx="1.5" />
    </svg>
  )
}

export function ListView({ size = 16, className, strokeWidth = 2 }: StrokeGlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="none" aria-hidden="true" className={className}>
      <path d="M8 6h13M8 12h13M8 18h13M3.5 6h.01M3.5 12h.01M3.5 18h.01" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round" />
    </svg>
  )
}

// --- Social icons (ported from SocialIcons.jsx) ---

export type SocialIconName = 'discord' | 'reddit' | 'github' | 'twitter'

const SOCIAL_ICONS: Record<SocialIconName, string> = {
  discord:
    'M19.6 4.6A18 18 0 0 0 15.1 3.2l-.2.4a16.7 16.7 0 0 1 4 1.3 15.1 15.1 0 0 0-12 0 16.7 16.7 0 0 1 4-1.3l-.2-.4A18 18 0 0 0 4.4 4.6 18.9 18.9 0 0 0 1.2 17.2 18.1 18.1 0 0 0 6.7 20l.4-.6a11.9 11.9 0 0 1-1.9-.9l.5-.4a12.9 12.9 0 0 0 10.6 0l.5.4a11.9 11.9 0 0 1-1.9.9l.4.6a18 18 0 0 0 5.5-2.8 18.9 18.9 0 0 0-3.2-12.6ZM8.4 14.6c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Zm7.2 0c-.9 0-1.6-.8-1.6-1.8s.7-1.8 1.6-1.8 1.6.8 1.6 1.8-.7 1.8-1.6 1.8Z',
  reddit:
    'M24 11.779c0-1.459-1.192-2.645-2.657-2.645-.715 0-1.363.286-1.84.746-1.81-1.191-4.259-1.949-6.971-2.046l1.483-4.669 4.016.941-.006.058c0 1.193.975 2.163 2.174 2.163 1.198 0 2.172-.97 2.172-2.163s-.975-2.164-2.172-2.164c-.92 0-1.704.574-2.021 1.379l-4.329-1.015c-.189-.046-.381.071-.44.249l-1.654 5.207c-2.758.082-5.25.838-7.077 2.041-.477-.46-1.125-.746-1.84-.746C1.192 9.134 0 10.32 0 11.779c0 1.07.646 1.991 1.564 2.398-.04.252-.06.507-.06.764 0 3.866 4.504 7.01 10.037 7.01s10.037-3.144 10.037-7.01c0-.256-.021-.51-.06-.76.917-.408 1.564-1.33 1.564-2.402zm-17.945 1.948c0-.894.728-1.622 1.622-1.622.893 0 1.621.728 1.621 1.622 0 .893-.728 1.621-1.621 1.621-.894.001-1.622-.727-1.622-1.621zm9.964 4.692c-1.21 1.21-3.526 1.296-4.21 1.296-.684 0-3.001-.086-4.209-1.296-.18-.18-.18-.474 0-.654.18-.18.474-.18.654 0 .76.76 2.388.927 3.555.927 1.168 0 2.795-.167 3.556-.927.18-.18.474-.18.654 0 .18.181.18.474 0 .654zm-.318-3.071c-.894 0-1.622-.728-1.622-1.621 0-.894.728-1.622 1.622-1.622.893 0 1.621.728 1.621 1.622 0 .893-.728 1.621-1.621 1.621z',
  github:
    'M12 2a10 10 0 0 0-3.2 19.5c.5.1.7-.2.7-.5v-1.7c-2.8.6-3.4-1.4-3.4-1.4-.4-1.2-1.1-1.5-1.1-1.5-.9-.6.1-.6.1-.6 1 .1 1.5 1 1.5 1 .9 1.6 2.4 1.1 3 .9.1-.7.4-1.1.6-1.4-2.2-.2-4.6-1.1-4.6-5a3.9 3.9 0 0 1 1-2.7 3.6 3.6 0 0 1 .1-2.7s.9-.3 2.8 1a9.6 9.6 0 0 1 5 0c1.9-1.3 2.8-1 2.8-1a3.6 3.6 0 0 1 .1 2.7 3.9 3.9 0 0 1 1 2.7c0 3.9-2.3 4.7-4.6 5 .4.3.7.9.7 1.8v2.7c0 .3.2.6.7.5A10 10 0 0 0 12 2Z',
  twitter:
    'M17.5 3h2.7l-5.9 6.7L21.3 21h-5.4l-4.3-5.6L6.7 21H4l6.3-7.2L3 3h5.5l3.9 5.1L17.5 3Zm-1 16.2h1.5L7.6 4.7H6l10.5 14.5Z',
}

export function SocialIcon({
  name,
  size = 20,
  className,
}: GlyphProps & { name: SocialIconName }): React.JSX.Element | null {
  const d = SOCIAL_ICONS[name]
  if (!d) return null
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d={d} />
    </svg>
  )
}

// --- StarWalletIcon (ported from StarWalletIcon.jsx) ---
// Source relies on external CSS for fill/stroke; bundled here via Glyphs.module.css
// so it paints with currentColor out of the box.

export function StarWalletIcon({ className }: { className?: string }): React.JSX.Element {
  return (
    <svg
      className={`${styles.starWallet} ${className ?? ''}`.trim()}
      xmlns="http://www.w3.org/2000/svg"
      width="82"
      height="75"
      viewBox="-10 0 115.6 97.6"
      aria-hidden="true"
    >
      <path d="M93.8,72.8v14c0,4.9-3.4,9-7.5,9h-77c-4.1,0-7.5-4-7.5-9" />
      <path d="M93.8,29.2V9.4c0-4.2-3.4-7.6-7.6-7.6H9.4c-4.2,0-7.6,3.4-7.6,7.6v76.4c0-2.3,2.1-4.8,6.1-4.8 c6.6,0,58.1,0,78.3,0c4.2,0,7.6-3.4,7.6-7.6V51.9" />
      <path d="M48,20.5l4.6,9.3c0.4,0.9,1.3,1.5,2.3,1.7l10.3,1.5c2.5,0.4,3.5,3.4,1.7,5.2l-7.4,7.2c-0.7,0.7-1,1.7-0.9,2.7 l1.8,10.2c0.4,2.5-2.2,4.4-4.4,3.2l-9.2-4.8c-0.9-0.5-1.9-0.5-2.8,0l-9.2,4.8c-2.2,1.2-4.8-0.7-4.4-3.2L32,48c0.2-1-0.2-2-0.9-2.7 l-7.4-7.2c-1.8-1.8-0.8-4.8,1.7-5.2l10.3-1.5c1-0.1,1.8-0.8,2.3-1.7l4.6-9.3C43.7,18.2,46.9,18.2,48,20.5z" />
      <path d="M100,50.8H86.8c-1.4,0-2.6-0.8-3.3-2l-3.3-6.2c-0.6-1.1-0.6-2.5,0-3.7l3.3-6.2c0.7-1.2,1.9-2,3.3-2H100 c2.1,0,3.8,1.7,3.8,3.8V47C103.8,49.1,102.1,50.8,100,50.8z" />
    </svg>
  )
}

// --- DclLogomark (ported from DclLogomark.jsx) ---

export function DclLogomark({ size = 40, className = '' }: GlyphProps): React.JSX.Element {
  return (
    <svg viewBox="0 0 54 54" width={size} height={size} fill="currentColor" aria-hidden="true" className={className}>
      <path d="M27.0988 54C31.4988 54 35.6988 53 39.3988 51.1H14.7988C18.4988 53 22.5988 54 27.0988 54Z" />
      <path d="M10.1988 48.2H43.6988C44.7988 47.3 45.9988 46.2 46.6988 45.4H7.19879C8.19879 46.4 9.19879 47.4 10.1988 48.2Z" />
      <path d="M28.7988 39.1H2.79879C3.39879 40.3 4.09879 41.4 4.79879 42.5H25.8988L28.7988 39.1Z" />
      <path d="M27.0988 0C12.2988 0 0.198792 12 0.198792 26.9C0.198792 30.2 0.798791 33.4 1.89879 36.3L12.2988 23.8L18.2988 16.5L32.8988 34.1L37.0988 29L48.0988 42.2H48.9988C52.0988 37.8 53.7988 32.5 53.7988 26.8C53.9988 12 41.8988 0 27.0988 0ZM18.1988 14.1C16.0988 14.1 14.4988 12.4 14.4988 10.4C14.4988 8.4 16.1988 6.7 18.1988 6.7C20.2988 6.7 21.8988 8.4 21.8988 10.4C21.8988 12.4 20.2988 14.1 18.1988 14.1ZM37.6988 26.2C33.5988 26.2 30.2988 22.9 30.2988 18.8C30.2988 14.7 33.5988 11.4 37.6988 11.4C41.7988 11.4 45.0988 14.7 45.0988 18.8C45.1988 22.9 41.7988 26.2 37.6988 26.2Z" />
      <path d="M18.9988 21.5V36.4H31.2988L18.9988 21.5Z" />
      <path d="M37.9988 42.7H44.9988L37.9988 34.2V42.7Z" />
    </svg>
  )
}

// --- ManaMark (ported from ManaMark.jsx) ---

export type ManaNetwork = 'ethereum' | 'polygon' | 'matic'

interface ManaGlyph {
  viewBox: string
  d: string
  rule: 'nonzero' | 'evenodd'
}

const MANA_GLYPHS: Record<'ethereum' | 'polygon', ManaGlyph> = {
  ethereum: {
    viewBox: '0 0 13 14',
    d: 'M9.864 6.952C9.864 4.968 8.264 3.32 6.28 3.32C4.296 3.32 2.696 4.968 2.696 6.952C2.696 8.92 4.296 10.52 6.28 10.52C8.264 10.52 9.864 8.92 9.864 6.952ZM9.032 6.952C9.032 8.456 7.688 9.688 6.28 9.688C4.728 9.688 3.528 8.488 3.528 6.952C3.528 5.432 4.728 4.152 6.28 4.152C7.704 4.152 9.032 5.432 9.032 6.952ZM12.248 10.424V3.544L6.28 0.12L0.312 3.544V10.424L6.28 13.848L12.248 10.424ZM11.192 9.832L6.296 12.632L1.368 9.832V4.184L6.28 1.336L11.192 4.184V9.832Z',
    rule: 'nonzero',
  },
  polygon: {
    viewBox: '0 0 24 24',
    d: 'M12 0L24 12L12 24L0 12L12 0ZM12.0002 3.36001L20.6402 12L12.0002 20.64L3.36023 12L12.0002 3.36001ZM12.0009 16.32C14.3868 16.32 16.3209 14.3859 16.3209 12C16.3209 9.61415 14.3868 7.68002 12.0009 7.68002C9.61507 7.68002 7.68094 9.61415 7.68094 12C7.68094 14.3859 9.61507 16.32 12.0009 16.32Z',
    rule: 'evenodd',
  },
}

export function ManaMark({
  size = 16,
  className,
  network = 'ethereum',
}: GlyphProps & { network?: ManaNetwork }): React.JSX.Element {
  const g = network === 'polygon' || network === 'matic' ? MANA_GLYPHS.polygon : MANA_GLYPHS.ethereum
  return (
    <svg
      className={className}
      viewBox={g.viewBox}
      width={size}
      height={size}
      fill="currentColor"
      aria-hidden="true"
    >
      <path d={g.d} fillRule={g.rule} clipRule={g.rule} />
    </svg>
  )
}
