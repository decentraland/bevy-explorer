// Avatar nametags — a name pill pinned above each player's head in 3D (the engine positions it in
// the render loop, so it tracks avatars smoothly; a DOM port can't, SDK7 scenes tick below the
// render rate). Styled pixel-for-pixel from unity-explorer's NametagStyle.uss: solid #161518 pill
// with a faint border, the name in its address-hashed palette colour, the DCL verified seal for
// claimed names, and a white-40% `#abcd` discriminator for unclaimed ones. Opacity is full within
// 20m and fades out to 40m, where the tag is culled (matching NametagPlacementSystem).
//
// When a player talks, the pill grows a CHAT BUBBLE line under the name (see setChatBubble + the
// chat domain) — restoring the previous SDK7 HUD's behaviour (bevy-ui-scene avatar-tags): the
// message shows for a few seconds, the newest one replacing any prior, with a bigger font for a
// single emoji and a brand-coloured border when the message @-mentions you.
import ReactEcs, { UiEntity } from '@dcl/react-ecs'
import {
  AvatarAnchorPointType,
  AvatarAttach,
  Billboard,
  CameraMode,
  CameraType,
  Material,
  MaterialTransparencyMode,
  MeshRenderer,
  PlayerIdentityData,
  Transform,
  UiCanvas,
  engine
} from '@dcl/sdk/ecs'
import { Color4, Vector3 } from '@dcl/sdk/math'
import { getPlayer, onEnterScene, onLeaveScene } from '@dcl/sdk/players'
import { ReactEcsRenderer } from '@dcl/sdk/react-ecs'
import type { Entity } from '@dcl/ecs'
import { fetchProfile } from './profile'

// UserNameColors.json — the 23-colour palette, indexed by FNV-1a(address) % 23. Kept in lockstep
// with the engine's name_color.rs so the in-world colour matches the point-at marker tint.
const PALETTE: ReadonlyArray<readonly [number, number, number]> = [
  [0.67138505, 0.38714847, 0.9433962], [0.8324557, 0.6273585, 1], [0.8716914, 0.3820755, 1],
  [1, 0.2028302, 0.9783837], [1, 0.3537736, 0.92354745], [1, 0.5235849, 0.79682314],
  [1, 0.7019608, 0.9433204], [1, 0.28773582, 0.30953965], [1, 0.4292453, 0.46791336],
  [1, 0.6367924, 0.66624165], [1, 0.5053185, 0.08018869], [1, 0.65705246, 0],
  [1, 0.8548728, 0], [1, 0.9431928, 0.6084906], [0.51564926, 0.8679245, 0],
  [0.6194137, 0.9607843, 0.121568605], [0.858401, 1, 0.5613208], [0, 1, 0.7287984],
  [0.5330188, 1, 0.9353978], [0.60784316, 0.8391339, 1], [0.60784316, 0.6527446, 1],
  [0.48584908, 0.7057166, 1], [0.2783019, 0.7820757, 1]
]
const NAME_COLORS: readonly Color4[] = PALETTE.map(([r, g, b]) => Color4.create(r, g, b, 1))

// unity-explorer colours (CommonStyles.uss): solid pill + border, white-40% wallet id.
const PILL_BG = Color4.create(22 / 255, 21 / 255, 24 / 255, 1) // --dcl-color-shadow #161518
const PILL_BORDER = Color4.create(67 / 255, 64 / 255, 74 / 255, 1) // --dcl-color-pale-black #43404A
const WALLET_ID_HEX = 'ffffff66' // <color=ffffff66> — white at 40%, the design's wallet-id grey

// Sizing: unity-explorer's NametagStyle.uss proportions (font 18, padding ~5/5/2/10, radius 15,
// badge 14, header gap 4, border 1), scaled into the texture and kept tight.
const FONT = 30
const PAD = { top: 4, bottom: 4, right: 6, left: 13 }
const RADIUS = 22
const BADGE = 22
const GAP = 6
const BORDER = 2

// Constant ON-SCREEN size: scale the plane ∝ distance so perspective cancels out and the pill stays
// the same size whatever the camera distance (unity-explorer's CalculateTagScale = fovScaleFactor ×
// distance). SIZE_FACTOR tunes apparent size; the distance is clamped so a transient bad reading can
// never fill the screen. DEFAULT_DIST is the size a tag is born at, before the per-frame system has
// measured it.
//
// The canvas is taller than the pill (640×NAME_BAND would be 4:1 for the name alone) to leave room
// BELOW the name for the chat bubble (the message line). The plane's HEIGHT is scaled by
// CANVAS_H/NAME_BAND so the name still renders at the same on-screen size as a name-only 4:1 tag —
// only the (transparent until someone talks) bubble area is extra. Width is unchanged (ASPECT·m), so
// the name keeps its width too.
const ASPECT = 4
const CANVAS_W = 640
const NAME_BAND = 160 // the name-only height the on-screen size is calibrated against (the old canvas)
const CANVAS_H = 440 // taller → room for a ~3-line bubble under the name
const Y_SCALE = CANVAS_H / NAME_BAND // plane height multiplier that preserves the name's on-screen size
const SIZE_FACTOR = 0.057 // a touch larger so names read better, esp. at distance
const MIN_DIST = 1
const FADE_START = 20
const MAX_DIST = 40
const DEFAULT_DIST = 4 // smaller birth size, before the sizing system measures the real distance
// The pill is attached at the avatar's NAME_TAG anchor (above the head), but getPlayer().position is
// the avatar's feet. For distant tags that offset is noise; for a CLOSE one the head is much nearer
// the camera than the feet, so sizing by the feet distance over-enlarges it. Measure to the head.
const NAMETAG_HEIGHT = 2.2
function tagScale(dist: number): Vector3 {
  const d = Math.max(MIN_DIST, Math.min(MAX_DIST, dist))
  const m = SIZE_FACTOR * d
  return Vector3.create(ASPECT * m, Y_SCALE * m, 1)
}

// --- chat bubble (the message line under the name) -----------------------------------------------
// When a player talks, their name pill grows a second line with the message — matching the previous
// SDK7 HUD (bevy-ui-scene avatar-tags) + unity-explorer: shown ~5s, REPLACED (not queued) on a new
// message, a bigger font for a single emoji, and a brand-coloured border for a message that @-mentions
// you. The chat domain feeds messages in via setChatBubble; an aging system expires them by dt (the
// scene sandbox has no wall clock).
const BUBBLE_TTL = 5 // seconds a bubble stays up; reset on each new message
const BUBBLE_MAX_W = 480 // message wrap width, in canvas px
const MSG_FONT = 30
const EMOJI_FONT = 62
const MSG_GAP = 2
const MSG_TRUNCATE = 100
const MENTION_BORDER = Color4.create(1, 45 / 255, 85 / 255, 1) // brand #ff2d55

type Bubble = { message: string; mention: boolean; ttl: number }
const bubbles = new Map<string, Bubble>() // lowercased address → its live bubble

/** Show (or refresh) a player's chat bubble. Called by the chat domain for each incoming message. */
export function setChatBubble(address: string, message: string, mention: boolean): void {
  const text = message.trim()
  if (address === '' || text === '') return
  bubbles.set(address.toLowerCase(), { message: text, mention, ttl: BUBBLE_TTL })
}

// Single-emoji detection (renders bigger, like the reference). Codepoint-range based so it works in
// the scene's JS runtime without Unicode property escapes; allows a trailing VS16 / skin-tone / ZWJ.
function isSingleEmoji(s: string): boolean {
  const cps = Array.from(s.trim())
  if (cps.length === 0 || cps.length > 3) return false
  let hasEmoji = false
  for (const c of cps) {
    const cp = c.codePointAt(0) ?? 0
    if (cp >= 0x1f000 || (cp >= 0x2600 && cp <= 0x27bf)) hasEmoji = true
    else if (cp === 0xfe0f || cp === 0x200d || (cp >= 0x1f3fb && cp <= 0x1f3ff)) continue // VS16 / ZWJ / skin tone
    else return false // a normal character → not emoji-only
  }
  return hasEmoji
}

// Truncate to a length without breaking the final word (… suffix), matching the reference bubble.
function truncateMessage(s: string, max: number): string {
  if (s.length <= max) return s
  const cut = s.slice(0, max)
  const sp = cut.lastIndexOf(' ')
  return (sp > max * 0.6 ? cut.slice(0, sp) : cut).replace(/\s+$/, '') + '…'
}

// 64-bit FNV-1a over the lowercase hex address — matches the engine + scene `simpleHash` exactly.
function fnv1a64(str: string): bigint {
  let hash = 2166136261n
  for (let i = 0; i < str.length; i++) {
    hash ^= BigInt(str.charCodeAt(i))
    hash = (hash * 16777619n) & 0xffffffffffffffffn
  }
  return hash
}
// Name colour, matching the engine's UserProfile::name_color() resolution so the pill agrees with the
// in-world point-at marker: a profile-set CUSTOM colour wins (claimed names only), else the address-
// hashed palette. (We keep colouring unclaimed names by hash rather than greying them, per the
// reference mobile nametags.) Custom colours come from the catalyst profile (resolveClaimed) and the
// hash is memoised — tagElement re-evaluates per texture render.
const customColorCache = new Map<string, Color4>()
const colorCache = new Map<string, Color4>()
function nameColor(userId: string): Color4 {
  const custom = customColorCache.get(userId)
  if (custom != null) return custom
  const key = userId.toLowerCase()
  const hit = colorCache.get(key)
  if (hit != null) return hit
  const c = NAME_COLORS[Number(fnv1a64(key) % BigInt(NAME_COLORS.length))]
  colorCache.set(key, c)
  return c
}

// hasClaimedName + custom name colour from the catalyst profile (async, cached); fall back to the
// name-suffix heuristic until it resolves so the badge / discriminator don't flicker on first sight.
const claimedCache = new Map<string, boolean>()
const pendingClaimed = new Set<string>()
function resolveClaimed(userId: string): void {
  if (claimedCache.has(userId) || pendingClaimed.has(userId)) return
  pendingClaimed.add(userId)
  fetchProfile(userId)
    .then((p) => {
      const av = p?.avatars?.[0]
      const name = getPlayer({ userId })?.name ?? ''
      const claimed = av?.hasClaimedName ?? !name.includes('#')
      claimedCache.set(userId, claimed)
      // The profile can set a custom name colour — engine logic applies it for claimed names only.
      const nc = av?.nameColor
      if (claimed && nc != null) customColorCache.set(userId, Color4.create(nc.r, nc.g, nc.b, 1))
    })
    .catch(() => undefined)
    .finally(() => pendingClaimed.delete(userId))
}

// The pill rendered into the tag's UiCanvas → texture. Re-evaluated each frame, so name/colour/
// claimed stay current and the own-avatar tag hides itself in first person.
function tagElement(userId: string): () => ReactEcs.JSX.Element | null {
  return function Tag(): ReactEcs.JSX.Element | null {
    const name = getPlayer({ userId })?.name
    if (name == null || name === '') return null
    const isSelf = userId === getPlayer()?.userId
    const firstPerson = CameraMode.get(engine.CameraEntity).mode === CameraType.CT_FIRST_PERSON
    if (isSelf && firstPerson) return null

    const isClaimed = claimedCache.get(userId) ?? !name.includes('#')
    const baseName = name.split('#')[0]
    // ONE text element so the name and #wallet-id sit tight (no inter-element gap — the Figma shows
    // them flush). Name is bold in its hash colour (the element's base colour); the wallet id uses
    // the engine's <color=RRGGBBAA> markup → white-40%, regular weight. Claimed names show no id.
    const value = isClaimed ? `<b>${baseName}</b>` : `<b>${baseName}</b><color=${WALLET_ID_HEX}>#${userId.slice(-4)}</color>`
    // The live chat bubble (if this player has spoken in the last few seconds). The pill is a COLUMN:
    // name row on top, message below — so the bubble reads as growing out of the nametag.
    const bubble = bubbles.get(userId.toLowerCase()) ?? null

    return (
      <UiEntity uiTransform={{ width: '100%', height: '100%', justifyContent: 'center', alignItems: 'center' }}>
        <UiEntity
          uiTransform={{
            flexDirection: 'column',
            alignItems: 'center',
            // PAD's right (6) is tuned for an unclaimed name's #id tail; the verified badge needs the
            // same breathing room as the left, else it sits flush against the rounded edge.
            padding: isClaimed ? { ...PAD, right: PAD.left } : PAD,
            borderRadius: RADIUS,
            borderWidth: BORDER,
            // @-mention: brand-coloured border (the reference highlights mentions of you).
            borderColor: bubble?.mention === true ? MENTION_BORDER : PILL_BORDER
          }}
          uiBackground={{ color: PILL_BG }}
        >
          <UiEntity uiTransform={{ flexDirection: 'row', alignItems: 'center' }}>
            <UiEntity uiText={{ value, fontSize: FONT, color: nameColor(userId), textAlign: 'middle-center' }} />
            {isClaimed && (
              <UiEntity
                uiTransform={{ width: BADGE, height: BADGE, margin: { left: GAP } }}
                uiBackground={{ textureMode: 'stretch', texture: { src: 'images/icon-verified.png' } }}
              />
            )}
          </UiEntity>
          {bubble != null && (
            <UiEntity
              uiTransform={{ maxWidth: BUBBLE_MAX_W, margin: { top: MSG_GAP } }}
              uiText={{
                value: truncateMessage(bubble.message, MSG_TRUNCATE),
                fontSize: isSingleEmoji(bubble.message) ? EMOJI_FONT : MSG_FONT,
                color: Color4.White(),
                textAlign: 'top-center',
                textWrap: 'wrap'
              }}
            />
          )}
        </UiEntity>
      </UiEntity>
    )
  }
}

// The pill material: the UiCanvas texture, alpha-blended + faintly emissive so it stays legible.
// `opacity` fades the whole pill by distance (albedo alpha for the body, emissive for the glow text).
function setTagMaterial(tag: Entity, opacity: number): void {
  const uiTexture = { tex: { $case: 'uiTexture' as const, uiTexture: { uiCanvasEntity: tag } } }
  Material.setPbrMaterial(tag, {
    transparencyMode: MaterialTransparencyMode.MTM_ALPHA_BLEND,
    // The bevy client defaults cast_shadows to true; explicit false marks the quad NotShadowCaster
    // so the pill doesn't cast a shadow (bevy-ui-scene PR #74).
    castShadows: false,
    texture: uiTexture,
    emissiveTexture: uiTexture,
    emissiveColor: Color4.White(),
    emissiveIntensity: 0.2 * opacity,
    albedoColor: Color4.create(1, 1, 1, opacity)
  })
}

// Two entities: an ANCHOR attached to the avatar's NAME_TAG point and billboarded to face the
// camera — its transform is owned entirely by the engine (AvatarAttach + Billboard run in the render
// loop), so the scene NEVER writes it. The PLANE is its child and carries the mesh/UiCanvas; the
// scene writes only the plane's local scale (for constant on-screen size). Splitting them is what
// kills the flicker — writing scale on the attached entity itself races the engine's per-frame
// position write and snaps the tag between the avatar and the origin.
function createTag(userId: string, isSelf: boolean, avatarPos?: { x: number; y: number; z: number }): Entity {
  const anchor = engine.addEntity()
  // Born AT the avatar (head height), not the origin: if AvatarAttach later fails to bind, the tag
  // sits on the avatar instead of stranding at world-origin where the distance-sizing balloons it into
  // a giant pill. When the attach DOES bind, the engine overwrites this position every frame.
  Transform.create(anchor, avatarPos != null ? { position: Vector3.create(avatarPos.x, avatarPos.y + NAMETAG_HEIGHT, avatarPos.z) } : {})
  // SELF attaches with NO avatarId → the engine binds it to the primary user directly (attach.rs),
  // which can never fail the avatar-shape-by-id lookup that was leaving the own-name tag frozen.
  AvatarAttach.create(anchor, isSelf ? { anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG } : { avatarId: userId, anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG })
  Billboard.create(anchor, {})

  // PLANE is a child: the scene writes only its local scale (constant size); the anchor's transform
  // is engine-owned (AvatarAttach + Billboard). Splitting them is what kills the scale-write flicker.
  const plane = engine.addEntity()
  Transform.create(plane, { parent: anchor, scale: tagScale(DEFAULT_DIST) })
  MeshRenderer.setPlane(plane)
  UiCanvas.create(plane, { width: CANVAS_W, height: CANVAS_H, color: Color4.Clear() })
  ReactEcsRenderer.setTextureRenderer(plane, tagElement(userId))
  setTagMaterial(plane, 1)
  return anchor
}

// getPlayer().position is the avatar's LIVE position; it's absent until the avatar has loaded and
// goes away when it despawns. We gate tag creation on it (so tags only appear once the avatar is in
// world — no first-frame glitch) and use it to drop+recreate tags across despawn/respawn.
function hasLivePosition(userId: string): boolean {
  return getPlayer({ userId })?.position != null
}
function distanceTo(userId: string, cam: { x: number; y: number; z: number }): number | null {
  const pos = getPlayer({ userId })?.position
  if (pos == null) return null
  // Measure to the name-tag anchor (above the head), not the feet, so a close tag isn't over-enlarged.
  return Math.hypot(pos.x - cam.x, pos.y + NAMETAG_HEIGHT - cam.y, pos.z - cam.z)
}

// Canonical address key. The SAME wallet can arrive in different casings from different sources
// (onEnterScene's player.userId, PlayerIdentityData.address, getPlayer().userId), and a case-sensitive
// Map key would treat `0xAbc…` and `0xabc…` as two players — spawning a SECOND tag whose AvatarAttach
// often fails to bind, so it strands at the origin and the distance-sizing blows it up (the duplicate
// "giant pill" bug). Keying every tag by the lowercased address makes the cache one-per-wallet. We
// still pass the engine the address in its ORIGINAL casing (what getPlayer/AvatarAttach already match).
const canon = (address: string): string => address.toLowerCase()

// Guard on globalThis (NOT a module-local) so a module re-eval / HMR that keeps the engine context
// can't start a SECOND copy of these systems beside the still-running first copy — two reconcilers,
// each with its own map, would spawn (and then fight over) duplicate tags.
const initFlag = globalThis as { __bevyNametagsStarted?: boolean }

export function initNametags(): void {
  if (initFlag.__bevyNametagsStarted === true) return // never stack the systems twice
  initFlag.__bevyNametagsStarted = true

  const tags = new Map<string, Entity>() // canonical address → anchor (exactly one tag per wallet)
  const uidOf = new Map<Entity, string>() // anchor → address (original casing, for getPlayer/distance)
  const selfId = (): string | undefined => {
    const id = getPlayer()?.userId
    return id != null && id !== '' ? id : undefined
  }

  const add = (userId: string): void => {
    // Only once the avatar actually has a position — otherwise AvatarAttach has nothing to bind to
    // and the tag would strand at the origin / get mis-placed on first load. Keyed by canon(address)
    // so a different casing of an already-tagged wallet can't create a duplicate.
    const key = canon(userId)
    if (userId === '' || tags.has(key) || !hasLivePosition(userId)) return
    const anchor = createTag(userId, key === canon(selfId() ?? ''), getPlayer({ userId })?.position)
    tags.set(key, anchor)
    uidOf.set(anchor, userId)
    resolveClaimed(userId)
  }
  const removeByKey = (key: string): void => {
    const anchor = tags.get(key)
    if (anchor == null) return
    engine.removeEntityWithChildren(anchor)
    tags.delete(key)
    uidOf.delete(anchor)
  }
  const remove = (userId: string): void => {
    removeByKey(canon(userId))
  }

  // THE hard guarantee against duplicate "stuck" pills: remove every nametag entity that doesn't
  // belong to a tracked tag. Two ways an orphan survives a removeEntityWithChildren that didn't fully
  // land (engine paused on alt+tab, child not yet linked, id churn): an untracked ANCHOR
  // (AvatarAttach+Billboard) OR a child PLANE (UiCanvas+MeshRenderer) whose parent anchor is gone /
  // untracked. We sweep BOTH. The global init guard keeps us the single authority, so this can't
  // fight another reconciler.
  const sweepOrphans = (): void => {
    const valid = new Set<Entity>(tags.values())
    for (const [anchor] of engine.getEntitiesWith(AvatarAttach, Billboard)) {
      if (!valid.has(anchor)) engine.removeEntityWithChildren(anchor)
    }
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      const parent = Transform.getOrNull(plane)?.parent
      if (parent == null || !valid.has(parent)) engine.removeEntityWithChildren(plane)
    }
  }
  sweepOrphans() // clear any orphans left by a previous scene instance before we start fresh

  onEnterScene((player) => {
    add(player.userId)
  })
  onLeaveScene((userId) => {
    remove(userId)
  })

  // Reconcile (~1s): presence = players whose avatar has a LIVE position (despawned avatars drop out,
  // so a frozen tag is removed and recreated fresh on respawn — the fix for tab-switch re-streams).
  // Adds missing, drops absent, and re-asserts the attachment so it re-links after a wearable rebuild
  // (the engine only re-links on Changed<AvatarAttach>). Self is always present and its attach can't
  // go stale (primary-user bind), so it's never re-asserted.
  let racc = 0
  engine.addSystem((dt: number) => {
    racc += dt
    if (racc < 1) return
    racc = 0

    sweepOrphans()
    const me = selfId()
    const meKey = me != null ? canon(me) : ''
    // Presence keyed by canon(address) → the address as the engine reported it (authoritative casing),
    // so the same wallet collapses to ONE entry however its address is cased across sources.
    const present = new Map<string, string>()
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) {
      if (data.address !== '' && hasLivePosition(data.address)) present.set(canon(data.address), data.address)
    }
    if (me != null && hasLivePosition(me)) present.set(meKey, me)
    for (const key of [...tags.keys()]) if (!present.has(key)) removeByKey(key)
    for (const [key, addr] of present) {
      add(addr)
      resolveClaimed(addr)
      if (key !== meKey) {
        const anchor = tags.get(key)
        if (anchor != null) {
          // Re-assert the attachment on the authoritative address — re-links a tag whose AvatarAttach
          // failed the avatar-shape-by-id lookup (the "stuck giant" case), and re-keys uidOf so the
          // size/distance system reads the correct avatar.
          AvatarAttach.createOrReplace(anchor, { avatarId: addr, anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG })
          uidOf.set(anchor, addr)
          // Keep the anchor's fallback position on the avatar: harmless while bound (the engine
          // overwrites it every frame), but if the attach never binds the tag follows the avatar
          // instead of stranding at world-origin and ballooning into a giant.
          const at = Transform.getMutableOrNull(anchor)
          const ap = getPlayer({ userId: addr })?.position
          if (at != null && ap != null) at.position = Vector3.create(ap.x, ap.y + NAMETAG_HEIGHT, ap.z)
        }
      }
    }
  })

  // Fast orphan sweep (~3Hz) so a stray pill is gone in a blink, not up to a full reconcile second.
  // Cheap (two entity queries); only acts when something is actually untracked.
  let sacc = 0
  engine.addSystem((dt: number) => {
    sacc += dt
    if (sacc < 0.3) return
    sacc = 0
    sweepOrphans()
  })

  // Age out chat bubbles: the scene sandbox has no wall clock, so expire by accumulated frame time.
  // tagElement reads the live `bubbles` map each render, so deleting one drops its message next frame.
  engine.addSystem((dt: number) => {
    if (bubbles.size === 0) return
    for (const [addr, b] of bubbles) {
      b.ttl -= dt
      if (b.ttl <= 0) bubbles.delete(addr)
    }
  })

  // Constant on-screen size (plane scale ∝ distance, clamped) + distance opacity. Writes ONLY the
  // plane child, so it never fights the engine's anchor transform; change-gated so a stationary scene
  // writes nothing. Runs at ~20Hz, not every frame: the anchor's POSITION is engine-driven (smooth at
  // render rate), and size only needs to re-measure occasionally — 20Hz keeps the per-frame entity
  // query + getPlayer cost off the hot path while staying visually identical for size.
  const lastScaleDist = new Map<number, number>()
  const lastOpacity = new Map<number, number>()
  let sacc2 = 0
  let opTick = 0
  engine.addSystem((dt: number) => {
    sacc2 += dt
    if (sacc2 < 0.05) return
    sacc2 = 0
    const cam = Transform.getOrNull(engine.CameraEntity)?.position
    const self = getPlayer()?.position
    if (cam == null || self == null) return
    // The camera always sits within a few metres of the local player once initialised. On first enter
    // its transform briefly reads the origin while avatars are already elsewhere → every distance
    // comes out huge (the "row of giant pills"). Skip sizing until the camera is actually near you.
    if (Math.hypot(cam.x - self.x, cam.y - self.y, cam.z - self.z) > 30) return

    opTick = (opTick + 1) % 3 // opacity ~every 3rd tick (~6.7Hz); it changes slowly + is heavier
    const doOpacity = opTick === 0
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      const anchor = Transform.getOrNull(plane)?.parent
      const uid = anchor != null ? uidOf.get(anchor) : undefined
      if (uid == null) {
        // Orphan plane (no tracked anchor): a leftover from a despawn/respawn that sweepOrphans
        // hasn't deleted yet. Collapse it NOW so its stale (possibly huge) scale can't flash as a
        // giant pill in the meantime — don't just skip it, which is what left the giant duplicates.
        const t = Transform.getMutableOrNull(plane)
        if (t != null && lastScaleDist.get(plane) !== -1) {
          t.scale = Vector3.create(0, 0, 0)
          lastScaleDist.set(plane, -1)
        }
        continue
      }
      const dist = distanceTo(uid, cam)
      const t = Transform.getMutableOrNull(plane)

      if (dist == null) {
        // avatar gone (despawn/loading) — hide immediately; reconcile will recreate on respawn
        if (lastScaleDist.get(plane) !== -1 && t != null) t.scale = Vector3.create(0, 0, 0)
        lastScaleDist.set(plane, -1)
        continue
      }

      const lsd = lastScaleDist.get(plane)
      if (lsd == null || Math.abs(lsd - dist) >= 0.05) {
        lastScaleDist.set(plane, dist)
        if (t != null) t.scale = tagScale(dist)
      }
      if (doOpacity) {
        const opacity = dist < MIN_DIST || dist > MAX_DIST ? 0 : Math.max(0, Math.min(1, (MAX_DIST - dist) / (MAX_DIST - FADE_START)))
        const prev = lastOpacity.get(plane)
        if (prev == null || Math.abs(prev - opacity) >= 0.02) {
          lastOpacity.set(plane, opacity)
          setTagMaterial(plane, opacity)
        }
      }
    }
  })
}
