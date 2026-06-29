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

// FIXED world size for the pill — the previous SDK scene used a constant (2,1,1) scale and never
// produced a giant. We deliberately do NOT resize per-frame by camera distance: that "constant on-screen
// size" trick scales the plane UP in world space as the camera pulls away, so a zoomed-out / overhead
// view balloons every tag, and a tag whose AvatarAttach failed to bind amplifies into the screen-filling
// giant. A fixed scale can't balloon, whatever the camera does or whether the attach bound.
const CANVAS_W = 640
const CANVAS_H = 440 // taller than the name → room for a chat bubble below it (transparent until used)
const FADE_START = 20
const MAX_DIST = 40 // past this the tag fades out entirely (kept for the opacity-only system below)
// The pill attaches at the avatar's NAME_TAG anchor (above the head) while getPlayer().position is the
// feet — offset the distance used for the opacity fade so a close tag isn't dimmed by the feet distance.
const NAMETAG_HEIGHT = 2.2
// Width ≈ the old 2-wide pill (a touch larger for our taller canvas); height keeps the canvas aspect so
// the texture isn't stretched. Applied ONCE at creation and never changed → no path to a giant.
const TAG_SCALE = Vector3.create(2.4, (2.4 * CANVAS_H) / CANVAS_W, 1)

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

// The OLD-scene approach (which worked): an ANCHOR positioned NATIVELY by the engine via AvatarAttach
// (smooth, render-rate, zero lag) + billboarded to face the camera, and a child PLANE carrying the
// mesh/UiCanvas. The scene NEVER writes the anchor's transform — only the plane's local scale (for our
// constant-on-screen sizing), so it never fights the engine's per-frame position write. The crucial
// invariant that keeps it from ever stranding into a giant lives in `add()`: we only ever create a tag
// for an avatar that is ACTUALLY PRESENT, so AvatarAttach always has a shape to bind to.
function createTag(userId: string): { anchor: Entity; plane: Entity } {
  const anchor = engine.addEntity()
  Transform.create(anchor) // owned by the engine (AvatarAttach + Billboard); the scene never writes it
  // Bind by address to the (present) foreign avatar. We NEVER tag the local player (see add()), so this
  // always has a real avatar shape to attach to — the no-avatarId "primary user" attach, which binds
  // inconsistently for a global scene and stranded into a giant centred pill, is gone.
  AvatarAttach.create(anchor, { avatarId: userId, anchorPointId: AvatarAnchorPointType.AAPT_NAME_TAG })
  Billboard.create(anchor, {})

  const plane = engine.addEntity()
  Transform.create(plane, { parent: anchor, scale: TAG_SCALE })
  MeshRenderer.setPlane(plane)
  UiCanvas.create(plane, { width: CANVAS_W, height: CANVAS_H, color: Color4.Clear() })
  ReactEcsRenderer.setTextureRenderer(plane, tagElement(userId))
  setTagMaterial(plane, 1)
  return { anchor, plane }
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
  // BUILD MARKER — to tell whether the engine is actually running this build. If you DON'T see this in
  // the console after a reload, the scene is stale (cached / not reloaded), not a code bug.
  console.log('[nametags] BUILD=noself+dedup+fixedscale+census')

  const tags = new Map<string, Entity>() // canonical address → anchor (exactly one tag per wallet)
  // PLANE entity → address. The size + sweep systems identify a tag's plane by its OWN id, NOT via its
  // parent anchor — because DCL recycles entity ids, so a removed anchor's id can be reused by a new
  // tag, making a STALE duplicate plane's parent resolve to a valid anchor and escape cleanup (the "two
  // kurd tags" bug). Keying off the plane's own id, a duplicate is always untracked → hidden + removed.
  const planeUid = new Map<Entity, string>()
  const anchorPlane = new Map<Entity, Entity>() // anchor → its plane (to drop both from tracking on removal)
  const selfId = (): string | undefined => {
    const id = getPlayer()?.userId
    return id != null && id !== '' ? id : undefined
  }

  // Is this avatar actually in-world (does it have a PlayerIdentityData entity)? The OLD-SCENE INVARIANT:
  // only ever create a tag for a present avatar, so AvatarAttach always has a shape to bind to and the tag
  // can never strand at the origin as a giant. (Our previous reconcile tagged comms-only/far avatars that
  // weren't really here → AvatarAttach had nothing to bind → stranded giant. That's the bug we're killing.)
  const avatarPresent = (key: string): boolean => {
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) if (canon(data.address) === key) return true
    return false
  }

  // Is there ALREADY a live nametag plane for this player? Checked before creating one so two sources
  // (onEnterScene + the deferred flush, or a re-enter) can never spawn a second plane even if the `tags`
  // map momentarily lost its entry — the duplicate "giant pill" was exactly a second plane for a player
  // who already had one. We scan live entities (not just `tags`) because that is the real source of truth.
  const hasLivePlane = (key: string): boolean => {
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      const uid = planeUid.get(plane)
      if (uid != null && canon(uid) === key) return true
    }
    return false
  }
  // Remove one tag PLANE and its parent anchor outright (used by the reconcile). Drops the plane→addr
  // entry; the reconcile rebuilds tags/anchorPlane wholesale, so we don't touch those here.
  const dropPlane = (plane: Entity): void => {
    const anchor = Transform.getOrNull(plane)?.parent as Entity | undefined
    planeUid.delete(plane)
    engine.removeEntityWithChildren(anchor ?? plane)
  }

  // We must NEVER create a tag for our own avatar (the old scene skipped self entirely — you don't see
  // your own nametag). selfId() is briefly null at boot, so an add() that arrives before we know who we
  // are is QUEUED, not created, then flushed once self is known. This is what guarantees self is filtered
  // out: the boot-race add() that used to slip through as a giant centred self pill now waits.
  const pendingAdds: string[] = []
  let knownSelf: string | undefined
  const add = (userId: string): void => {
    const key = canon(userId)
    if (userId === '' || tags.has(key) || hasLivePlane(key)) return
    const me = knownSelf ?? selfId()
    if (me == null) {
      if (!pendingAdds.includes(userId)) pendingAdds.push(userId)
      return
    }
    if (canon(me) === key) return // never tag the local player
    if (!avatarPresent(key)) return
    const { anchor, plane } = createTag(userId)
    tags.set(key, anchor)
    planeUid.set(plane, userId)
    anchorPlane.set(anchor, plane)
    resolveClaimed(userId)
  }
  const removeByKey = (key: string): void => {
    const anchor = tags.get(key)
    if (anchor == null) return
    engine.removeEntityWithChildren(anchor)
    tags.delete(key)
    const plane = anchorPlane.get(anchor)
    if (plane != null) {
      planeUid.delete(plane)
      anchorPlane.delete(anchor)
    }
  }
  const remove = (userId: string): void => removeByKey(canon(userId))

  // Remove any stray nametag PLANE that isn't a tracked tag (identify by the plane's OWN id — id recycling
  // makes a stale plane's parent look valid). Cheap; only acts when something is actually untracked.
  const sweepOrphans = (): void => {
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      if (!planeUid.has(plane)) engine.removeEntityWithChildren(plane)
    }
  }
  sweepOrphans() // clear any orphans left by a previous scene instance before we start fresh

  // LIFECYCLE (old-scene style): create a tag when a player ENTERS the scene (engine event → the avatar is
  // really rendered here, so the attach binds), remove it when they leave. No reconcile re-creating tags
  // from comms presence — that was the source of the stranded giants.
  onEnterScene((player) => add(player.userId))
  onLeaveScene((userId) => remove(userId))

  // Learn the local player's id as soon as it's available, then flush the adds that arrived before we
  // knew it (deferred so they couldn't accidentally tag our own avatar). Stops once self is known.
  engine.addSystem(() => {
    if (knownSelf != null) return
    const me = selfId()
    if (me == null) return
    knownSelf = me
    for (const u of pendingAdds.splice(0)) add(u)
  })

  // RECONCILE (~1s): the authoritative backstop enforcing the invariant the user spelled out — EXACTLY
  // ONE nametag per present foreign player, and NONE for the local player or anyone who has left. The
  // incremental path can leave a SECOND tracked plane for a player (id recycling, or a tag made before the
  // avatar finished rendering so its anchor never bound) — that is the giant duplicate, and because its
  // key is "present" a removal-only cleanup never touches it. Here we scan every live tag plane, group by
  // player, drop self/absent/orphan planes, and for any player carrying MORE THAN ONE plane we drop them
  // all and recreate a single fresh tag (the avatar is fully present by now, so the new attach binds — no
  // re-stranding). Tracking maps are rebuilt from the survivors so add() never spawns a duplicate again.
  let racc = 0
  engine.addSystem((dt: number) => {
    racc += dt
    if (racc < 1) return
    racc = 0
    const me = selfId()
    const meKey = me != null ? canon(me) : undefined
    const present = new Set<string>()
    for (const [, data] of engine.getEntitiesWith(PlayerIdentityData)) if (data.address !== '') present.add(canon(data.address))

    const byKey = new Map<string, Entity[]>()
    let orphans = 0
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      const uid = planeUid.get(plane)
      if (uid == null) { orphans++; dropPlane(plane); continue } // orphan plane (untracked) → gone
      const key = canon(uid)
      const arr = byKey.get(key)
      if (arr != null) arr.push(plane)
      else byKey.set(key, [plane])
    }
    // DIAGNOSTIC: one compact line/sec — `addr6:planeCount` per player, who is self, how many orphans.
    // Tells us if a duplicate is 2 planes for one address, 2 addresses, or a self tag slipping through.
    console.log(`[nametags] census self=${meKey?.slice(-6) ?? 'none'} orphans=${orphans} planes=${[...byKey.entries()].map(([k, ps]) => `${k.slice(-6)}:${ps.length}`).join(' ') || '(none)'}`)

    tags.clear()
    anchorPlane.clear()
    for (const [key, planes] of byKey) {
      if (key === meKey || !present.has(key)) {
        for (const p of planes) dropPlane(p) // self, or avatar left → no tag at all
        continue
      }
      if (planes.length > 1) {
        const userId = planeUid.get(planes[0]) // keep original casing to recreate
        for (const p of planes) dropPlane(p) // nuke the duplicates…
        if (userId != null && avatarPresent(key)) {
          const { anchor, plane } = createTag(userId) // …and put back exactly one, freshly bound
          tags.set(key, anchor)
          planeUid.set(plane, userId)
          anchorPlane.set(anchor, plane)
        }
        continue
      }
      // exactly one plane → keep it; just make the tracking maps agree with reality
      const only = planes[0]
      const anchor = Transform.getOrNull(only)?.parent as Entity | undefined
      if (anchor != null) {
        tags.set(key, anchor)
        anchorPlane.set(anchor, only)
      }
    }
  })

  // Age out chat bubbles: the scene sandbox has no wall clock, so expire by accumulated frame time.
  engine.addSystem((dt: number) => {
    if (bubbles.size === 0) return
    for (const [addr, b] of bubbles) {
      b.ttl -= dt
      if (b.ttl <= 0) bubbles.delete(addr)
    }
  })

  // VISIBILITY ONLY (no per-frame resize — the scale is FIXED, set once at creation). AvatarAttach owns
  // the anchor's position natively. Each tick we only: hide a plane whose avatar has no live position (a
  // stale/duplicate plane → scale 0), restore a valid one to the FIXED scale, and fade opacity by distance.
  const hidden = new Map<number, boolean>()
  const lastOpacity = new Map<number, number>()
  let sacc2 = 0
  let opTick = 0
  engine.addSystem((dt: number) => {
    sacc2 += dt
    if (sacc2 < 0.1) return
    sacc2 = 0
    const cam = Transform.getOrNull(engine.CameraEntity)?.position
    if (cam == null) return
    opTick = (opTick + 1) % 3
    const doOpacity = opTick === 0
    for (const [plane] of engine.getEntitiesWith(UiCanvas, MeshRenderer)) {
      const t = Transform.getMutableOrNull(plane)
      const uid = planeUid.get(plane)
      // Untracked plane (stale/duplicate) or avatar with no live position → don't render it.
      const avPos = uid != null ? getPlayer({ userId: uid })?.position : undefined
      if (avPos == null) {
        if (t != null && hidden.get(plane) !== true) {
          t.scale = Vector3.create(0, 0, 0)
          hidden.set(plane, true)
        }
        continue
      }
      if (t != null && hidden.get(plane) !== false) {
        t.scale = TAG_SCALE // valid again (or first sight) → fixed size, never distance-scaled
        hidden.set(plane, false)
      }
      if (doOpacity) {
        const dist = Math.hypot(avPos.x - cam.x, avPos.y + NAMETAG_HEIGHT - cam.y, avPos.z - cam.z)
        const opacity = dist > MAX_DIST ? 0 : Math.max(0, Math.min(1, (MAX_DIST - dist) / (MAX_DIST - FADE_START)))
        const prev = lastOpacity.get(plane)
        if (prev == null || Math.abs(prev - opacity) >= 0.02) {
          lastOpacity.set(plane, opacity)
          setTagMaterial(plane, opacity)
        }
      }
    }
  })
}
