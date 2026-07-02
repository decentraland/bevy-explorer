// Design-system storybook — the living reference for tokens + primitives, matching
// the Explorer 2.0 Figma. Open with ?showcase=1. Add every new primitive here.

import { Button } from './Button'
import { IconButton } from './IconButton'
import { useState } from 'react'
import { ControlButton } from './ControlButton'
import { Tooltip } from './Tooltip'
import { Toggle } from './Toggle'
import { Slider } from './Slider'
import { Select } from './Select'
import { Panel } from './Panel'
import { DclLogo } from './DclLogo'
import { Avatar } from './Avatar'
import { WearableCard, type Rarity } from './WearableCard'
import type { IconName } from './icons'
import { ChatBubble, DaySeparator, MemberRow } from '../features/chat/Chat'
import { EmoteSlot } from '../features/emotes/EmoteSlot'
import { catalystThumbUrl } from '../lib/identity'

const EMOTE_RARITIES = ['base', 'common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic', 'unique', 'exotic']
const SLOT_THUMB = catalystThumbUrl('urn:decentraland:off-chain:base-emotes:raisehand')

const NAV: { icon: IconName; label: string }[] = [
  { icon: 'profile', label: 'Profile' },
  { icon: 'notifications', label: 'Notifications' },
  { icon: 'map', label: 'Map' },
  { icon: 'communities', label: 'Communities' },
  { icon: 'backpack', label: 'Backpack' },
  { icon: 'settings', label: 'Settings' },
  { icon: 'mic', label: 'Voice chat' },
  { icon: 'friends', label: 'Friends' },
  { icon: 'chat', label: 'Chat' },
  { icon: 'emotes', label: 'Emotes' },
  { icon: 'help', label: 'Help' }
]

// DCL named palette (Explorer 2.0). token = CSS var when one exists.
const PALETTE: { name: string; hex: string; token?: string; dark?: boolean }[] = [
  { name: 'Ruby', hex: '#ff2d55', token: '--brand' },
  { name: 'Snow', hex: '#fcfcfc', token: '--text', dark: true },
  { name: 'Shadow', hex: '#161518', token: '--panel' },
  { name: 'Yellow', hex: '#ffc95b', token: '--gold', dark: true },
  { name: 'Pearl', hex: '#ecebed', token: '--pearl', dark: true },
  { name: 'Green', hex: '#30cd00', token: '--green', dark: true },
  { name: 'Lavender', hex: '#c640cd', token: '--lavender' },
  { name: 'Purple', hex: '#982de2', token: '--purple' },
  { name: 'Red', hex: '#ff0000', token: '--red' }
]

// Rarity name colors (used for player names / avatars). No tokens — domain palette.
const RARITY: { name: string; hex: string }[] = [
  { name: 'Common', hex: '#73d3d3' },
  { name: 'Common Light', hex: '#acf8f8' },
  { name: 'Uncommon', hex: '#ff8362' },
  { name: 'Epic Light', hex: '#81e1ff' },
  { name: 'Legendary', hex: '#a14bf3' },
  { name: 'Legendary Light', hex: '#e8b9ff' },
  { name: 'Mythic', hex: '#ff4bed' },
  { name: 'Unique', hex: '#fea217' },
  { name: 'Exotic', hex: '#caff73' },
  { name: 'Orange', hex: '#ff7439' },
  { name: 'Melon', hex: '#ffa25a' },
  { name: 'Periwinkle', hex: '#a0abff' }
]

const TYPE: { name: string; token: string; weight: number }[] = [
  { name: 'Display', token: '--fs-display', weight: 800 },
  { name: 'Title', token: '--fs-title', weight: 700 },
  { name: 'Large', token: '--fs-lg', weight: 600 },
  { name: 'Body (md)', token: '--fs-md', weight: 500 },
  { name: 'Small', token: '--fs-sm', weight: 400 },
  { name: 'XSmall', token: '--fs-xs', weight: 400 }
]

const RADII: { name: string; token: string }[] = [
  { name: 'control', token: '--r-control' },
  { name: 'card', token: '--r-card' },
  { name: 'panel', token: '--r-panel' },
  { name: 'pill', token: '--r-pill' }
]

const wrap: React.CSSProperties = {
  // #root is overflow:hidden (HUD app shouldn't scroll), so the Showcase is its own
  // scroll container — full height with internal overflow.
  height: '100%',
  overflowY: 'auto',
  padding: 40,
  display: 'flex',
  flexDirection: 'column',
  gap: 36,
  background: '#0e0d10',
  color: 'var(--text)',
  fontFamily: 'var(--font-family)'
}
const row: React.CSSProperties = { display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }
const grid: React.CSSProperties = { display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(132px, 1fr))', gap: 12 }
const h: React.CSSProperties = {
  fontSize: 'var(--fs-xs)',
  textTransform: 'uppercase',
  letterSpacing: '.08em',
  color: 'var(--ink-45)',
  margin: '0 0 14px'
}

function Section({ title, children }: { title: string; children: React.ReactNode }): React.JSX.Element {
  return (
    <section>
      <p style={h}>{title}</p>
      {children}
    </section>
  )
}

function Swatch({ name, hex, token, dark }: { name: string; hex: string; token?: string; dark?: boolean }): React.JSX.Element {
  return (
    <div style={{ borderRadius: 'var(--r-card)', overflow: 'hidden', border: '1px solid var(--white-10)' }}>
      <div style={{ background: hex, height: 64, display: 'flex', alignItems: 'flex-end', padding: 8, color: dark ? '#161518' : '#fcfcfc', fontSize: 11, fontWeight: 700 }}>
        {hex.toUpperCase()}
      </div>
      <div style={{ padding: '8px 10px', background: 'var(--fill-2)' }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>{name}</div>
        {token && <div style={{ fontSize: 11, color: 'var(--ink-45)', fontFamily: 'var(--font-mono)' }}>{token}</div>}
      </div>
    </div>
  )
}

export function Showcase(): React.JSX.Element {
  const [on, setOn] = useState(true)
  const [vol, setVol] = useState(60)
  const [res, setRes] = useState('1080')
  return (
    <div style={wrap}>
      <h1 style={{ margin: 0, fontSize: 'var(--fs-title)' }}>Design system · Explorer 2.0</h1>

      <Section title="Palette (tokens)">
        <div style={grid}>
          {PALETTE.map((c) => (
            <Swatch key={c.name} {...c} />
          ))}
        </div>
      </Section>

      <Section title="Rarity name colors">
        <div style={grid}>
          {RARITY.map((c) => (
            <Swatch key={c.name} {...c} dark />
          ))}
        </div>
      </Section>

      <Section title="Typography (Inter)">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {TYPE.map((t) => (
            <div key={t.name} style={{ display: 'flex', alignItems: 'baseline', gap: 16 }}>
              <span style={{ width: 96, fontSize: 11, color: 'var(--ink-45)', fontFamily: 'var(--font-mono)' }}>{t.token}</span>
              <span style={{ fontSize: `var(${t.token})`, fontWeight: t.weight }}>{t.name} — Decentraland</span>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Radii">
        <div style={row}>
          {RADII.map((r) => (
            <div key={r.name} style={{ textAlign: 'center' }}>
              <div style={{ width: 72, height: 72, background: 'var(--fill-4)', borderRadius: `var(${r.token})`, border: '1px solid var(--white-10)' }} />
              <div style={{ fontSize: 11, marginTop: 6 }}>{r.name}</div>
              <div style={{ fontSize: 11, color: 'var(--ink-45)', fontFamily: 'var(--font-mono)' }}>{r.token}</div>
            </div>
          ))}
        </div>
      </Section>

      <Section title="Button">
        <div style={row}>
          <Button>Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="ghost">Ghost</Button>
          <Button disabled>Disabled</Button>
        </div>
        <div style={{ ...row, marginTop: 12 }}>
          <Button size="sm">Small</Button>
          <Button size="md">Medium</Button>
          <Button size="lg">Large</Button>
        </div>
      </Section>

      <Section title="Tooltip (hover the chips)">
        <div style={{ ...row, gap: 16 }}>
          <Tooltip label="Friends" shortcut="L">
            <span style={{ padding: '8px 14px', borderRadius: 10, background: 'var(--fill-4)' }}>right</span>
          </Tooltip>
          <Tooltip label="Settings" shortcut="P" side="top">
            <span style={{ padding: '8px 14px', borderRadius: 10, background: 'var(--fill-4)' }}>top</span>
          </Tooltip>
        </div>
      </Section>

      <Section title="ControlButton">
        <div style={row}>
          <ControlButton aria-label="ghost square">✕</ControlButton>
          <ControlButton variant="solid" aria-label="solid square">✕</ControlButton>
          <ControlButton shape="circle" aria-label="circle">＋</ControlButton>
          <ControlButton shape="pill">＋ Pill</ControlButton>
          <ControlButton active aria-label="active">✓</ControlButton>
          <ControlButton size="sm" aria-label="small">✕</ControlButton>
        </div>
      </Section>

      <Section title="Form controls">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16, maxWidth: 320 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span style={{ fontSize: 14 }}>Fullscreen</span>
            <Toggle checked={on} onChange={setOn} aria-label="Fullscreen" />
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontSize: 14, width: 60 }}>Volume</span>
            <Slider value={vol} onChange={setVol} aria-label="Volume" />
            <span style={{ fontSize: 13, color: 'var(--ink-45)', width: 32 }}>{vol}</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
            <span style={{ fontSize: 14 }}>Resolution</span>
            <Select
              value={res}
              onChange={setRes}
              aria-label="Resolution"
              options={[
                { value: '720', label: '1280 × 720' },
                { value: '1080', label: '1920 × 1080' },
                { value: '1440', label: '2560 × 1440' }
              ]}
            />
          </div>
        </div>
      </Section>

      <Section title="Emote Wheel Slot (Figma 10386-4701)">
        <div style={{ display: 'flex', gap: 24, alignItems: 'flex-start', flexWrap: 'wrap', padding: 16, background: '#3a3550', borderRadius: 12 }}>
          <EmoteSlot thumbnail={SLOT_THUMB} rarity="legendary" width={220} />
          <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            {EMOTE_RARITIES.map((r) => (
              <div key={r} style={{ textAlign: 'center' }}>
                <EmoteSlot thumbnail={SLOT_THUMB} rarity={r} width={96} />
                <div style={{ fontSize: 11, color: 'var(--ink-7)', marginTop: 4, textTransform: 'capitalize' }}>{r}</div>
              </div>
            ))}
          </div>
        </div>
      </Section>

      <Section title="DPI · HUD scale (Unity CanvasScaler)">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 14, maxWidth: 560 }}>
          <p style={{ fontSize: 13, color: 'var(--ink-7)', margin: 0, lineHeight: 1.5 }}>
            The HUD scales with viewport height against a 1080 reference, clamped 0.6–1.3 — the same
            curve as Unity&apos;s “Scale With Screen Size”. <code>innerHeight</code> is logical px
            (device px ÷ DPR), so a Retina panel scales identically to Unity on the same display.
          </p>
          <div style={{ display: 'flex', gap: 20, flexWrap: 'wrap' }}>
            {['720', '900', '1080', '1440'].map((h) => {
              const s = Math.min(1.3, Math.max(0.6, Number(h) / 1080))
              const sel = h === res
              return (
                <div key={h} style={{ textAlign: 'center' }}>
                  <div style={{ fontSize: 12, color: sel ? 'var(--text)' : 'var(--ink-45)', marginBottom: 6, fontFamily: 'var(--font-mono)' }}>
                    {h}p → ×{s.toFixed(3)}
                  </div>
                  <div style={{ width: 132, height: 84, display: 'flex', alignItems: 'flex-start' }}>
                    <div
                      style={{
                        width: 100,
                        padding: '8px 10px',
                        borderRadius: 10,
                        background: 'var(--fill-4)',
                        border: sel ? '1px solid var(--brand)' : '1px solid var(--white-10)',
                        transform: `scale(${s})`,
                        transformOrigin: 'top left',
                        fontSize: 13,
                        fontWeight: 600
                      }}
                    >
                      HUD card
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
          <div style={{ fontSize: 12, color: 'var(--ink-45)', fontFamily: 'var(--font-mono)' }}>
            Live viewport: {window.innerHeight}px → ×{Math.min(1.3, Math.max(0.6, window.innerHeight / 1080)).toFixed(3)}
          </div>
        </div>
      </Section>

      <Section title="IconButton (sidebar rail)">
        <Panel style={{ width: 60, padding: 8, display: 'flex', flexDirection: 'column', gap: 4 }}>
          {NAV.map((n, i) => (
            <IconButton key={n.icon} icon={n.icon} label={n.label} active={i === 8} badge={i === 1 ? 3 : undefined} />
          ))}
        </Panel>
      </Section>

      <Section title="Panel">
        <Panel style={{ padding: 18, maxWidth: 360 }}>
          <div style={{ fontWeight: 700, marginBottom: 6 }}>Genesis Plaza</div>
          <div style={{ color: 'var(--ink-7)', fontSize: 'var(--fs-sm)' }}>A surface for HUD widgets and menus.</div>
        </Panel>
      </Section>

      <Section title="Brand">
        <div style={{ ...row, gap: 16 }}>
          <DclLogo size={24} />
          <DclLogo size={32} />
          <DclLogo size={48} />
        </div>
      </Section>

      <Section title="Avatar">
        <div style={{ ...row, gap: 16 }}>
          <Avatar name="Mojito" color="#73d3d3" size={28} />
          <Avatar name="Pravus" color="#ff4bed" size={40} status="online" />
          <Avatar
            name="Mojito"
            size={40}
            status="online"
            src="https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png"
          />
          <Avatar name="Away" color="#a0abff" size={40} status="away" />
        </div>
      </Section>

      <Section title="WearableCard (backpack)">
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, 84px)', gap: 10 }}>
          {(['common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic', 'unique', 'exotic', 'base'] as Rarity[]).map((r, i) => (
            <div key={r} style={{ width: 84 }}>
              <WearableCard rarity={r} equipped={i === 4} isNew={i === 1} count={i === 7 ? 2 : undefined} />
            </div>
          ))}
        </div>
      </Section>

      <Section title="Chat components">
        <div style={{ display: 'flex', gap: 24, flexWrap: 'wrap', alignItems: 'flex-start' }}>
          <div style={{ width: 300, display: 'flex', flexDirection: 'column', gap: 8, padding: 12, background: 'rgba(19,19,19,0.6)', borderRadius: 14 }}>
            <DaySeparator ts={Date.now()} />
            <ChatBubble
              line={{ id: 1, sender: '0x5854cce95d5e25817b41f4c41f06b695a83bc495', message: 'gm everyone 👋 welcome to the plaza', channel: 'Nearby', ts: Date.now() }}
              name="Mojito"
            />
            <ChatBubble
              line={{ id: 2, sender: 'system', message: 'Type /help for available commands.', channel: 'System', ts: Date.now() }}
              name="DCL System"
            />
          </div>
          <div style={{ width: 300, display: 'flex', flexDirection: 'column', gap: 2, padding: 8, background: 'rgba(12,11,14,0.97)', borderRadius: 14 }}>
            <MemberRow member={{ address: '0x5854cce95d5e25817b41f4c41f06b695a83bc495', name: 'Mojito', picture: 'https://profile-images.decentraland.org/entities/bafkreid5btlh76opew65hxu6dtkdo6ybqhymdof6vrrmjy2p5a74oy4huq/face.png' }} />
            <MemberRow member={{ address: '0x6723dcb07f3ca735223cd1c0acfa62dd994a1bb4', name: 'Sharknado#a1b2' }} />
            <MemberRow member={{ address: '0x1e105bb213754519903788022b962fe2b9c4b263', name: 'Pravus' }} />
          </div>
        </div>
      </Section>
    </div>
  )
}
