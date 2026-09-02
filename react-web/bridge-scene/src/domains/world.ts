// World: current parcel + teleport, the live player pose for the minimap, the realm kind,
// and the mic state.
//   from: @dcl/sdk getPlayer().position (parcel), Transform of the player/camera entities
//         (pose), RestrictedActions.teleportTo, Runtime.getRealm,
//         BevyApi.getMicState() / setMicEnabled().
import { Transform, engine } from '@dcl/sdk/ecs'
import { Quaternion } from '@dcl/sdk/math'
import { getPlayer } from '@dcl/sdk/players'
import { teleportTo, changeRealm } from '~system/RestrictedActions'
import { getRealm } from '~system/Runtime'
import { BevyApi } from '../bevy-api'
import type { Ctx } from '../bridge'
import { throttleByDt, singleFlight } from '../system-helpers'

// Pose stream: ~20/s is smooth enough for the minimap once React interpolates between
// samples, and keeps the bridge far quieter than a per-frame push.
const POSE_INTERVAL = 0.05
// Don't re-send while the player stands still: 5 cm and half a degree are both below
// what a minimap can show.
const POSE_EPSILON_M = 0.05
const POSE_EPSILON_DEG = 0.5

// Scene lookup for the minimap header. The RPC only fires on a parcel change, so an idle tick
// costs one coordinate compare. Attempts are spaced by the poll interval, giving a retry budget
// of INTERVAL × ATTEMPTS for a scene that hasn't registered yet after entering or teleporting.
const SCENE_POLL_INTERVAL = 0.3
const SCENE_LOOKUP_ATTEMPTS = 3

// A realm is a World (rather than Genesis City) when it's served by a worlds content server
// or named like `foo.dcl.eth`. Worlds have no map tiles, so the minimap falls back to the
// engine-rendered Camera style there and drops its Genesis City markers.
//
// The two checks are not redundant — they come from different fields, and either one alone
// misses cases: a world reached by name (`?realm=welcomeguides.dcl.eth`) need not carry the
// content server in its base url. Same pair bevy-ui-scene's realm-change check used.
function realmIsWorld(baseUrl: string, realmName: string): boolean {
  return baseUrl.includes('worlds-content-server') || realmName.endsWith('.eth')
}

// Echo a "DCL System" line into the React chat (empty sender → system member). Used to relay
// slash-command feedback (/commands output, /reload status) that isn't broadcast to other players.
function pushSystem(ctx: Ctx, message: string): void {
  ctx.send({ kind: 'chat', chat: { sender: '', message, channel: 'Nearby' } })
}

export function registerWorld(ctx: Ctx): void {
  ctx.on('getMap', () => {
    const pos = getPlayer()?.position
    ctx.send({ kind: 'mapState', x: Math.floor((pos?.x ?? 0) / 16), y: Math.floor((pos?.z ?? 0) / 16) })
  })

  ctx.on('teleport', (msg) => {
    teleportTo({ worldCoordinates: { x: msg.x, y: msg.y } }).catch((e: unknown) => {
      console.error('[world] teleport failed', e)
    })
  })

  // Travel to a world/realm (e.g. boedo.dcl.eth). The engine auto-grants ChangeRealm for our
  // super-user scene, so the React HUD owns the confirmation prompt.
  ctx.on('changeRealm', (msg) => {
    changeRealm({ realm: msg.realm }).catch((e: unknown) => {
      console.error('[world] changeRealm failed', e)
    })
  })

  // `/reload` — reload the scene the player is standing in, resolved by parcel from liveSceneInfo.
  // Never the super-user bridge (isSuper filtered out) and never reload-all, so the HUD survives.
  ctx.on('reloadScene', () => {
    const op = BevyApi.consoleCommand
    if (op == null) {
      pushSystem(ctx, 'Reload is not available.')
      return
    }
    const pos = getPlayer()?.position
    const px = Math.floor((pos?.x ?? 0) / 16)
    const py = Math.floor((pos?.z ?? 0) / 16)
    BevyApi.liveSceneInfo()
      .then(async (scenes) => {
        const current = scenes.find((s) => !s.isSuper && (s.parcels ?? []).some((p) => p.x === px && p.y === py))
        if (current == null) {
          pushSystem(ctx, 'Could not find the current scene to reload.')
          return
        }
        await op('reload', [current.hash]).then(() => { pushSystem(ctx, `Reloading ${current.title || current.hash}…`); });
      })
      .catch((e: unknown) => {
        console.error('[world] reload failed', e)
        pushSystem(ctx, 'Reload failed.')
      })
  })

  // Engine console passthrough (`/commands` → `help`, and any chat `/command` the page forwards).
  // The output — or the rejection text (unknown command, usage error) — goes back as a
  // `consoleReply`; the page decides how to render it (it filters its own hidden commands).
  ctx.on('consoleCommand', (msg) => {
    const op = BevyApi.consoleCommand
    if (op == null) {
      pushSystem(ctx, 'Engine console is not available.')
      return
    }
    const args = msg.args ?? []
    op(msg.command, args)
      .then((output) => {
        ctx.send({ kind: 'consoleReply', command: msg.command, args, ok: true, output })
      })
      .catch((e: unknown) => {
        ctx.send({ kind: 'consoleReply', command: msg.command, args, ok: false, output: e instanceof Error ? e.message : String(e) })
      })
  })

  ctx.on('setMic', (msg) => {
    BevyApi.setMicEnabled(msg.enabled)
  })

  // Player pose → the minimap (position in metres, avatar and camera yaw in degrees).
  let poseAcc = 0
  let lastX = NaN
  let lastZ = NaN
  let lastYaw = NaN
  let lastCamYaw = NaN
  ctx.push((dt) => {
    poseAcc += dt
    if (poseAcc < POSE_INTERVAL) return
    poseAcc = 0
    const pos = getPlayer()?.position
    if (pos == null) return
    const playerT = Transform.getOrNull(engine.PlayerEntity)
    const camT = Transform.getOrNull(engine.CameraEntity)
    const yaw = playerT == null ? 0 : Quaternion.toEulerAngles(playerT.rotation).y
    const camYaw = camT == null ? 0 : Quaternion.toEulerAngles(camT.rotation).y
    const still =
      Math.abs(pos.x - lastX) < POSE_EPSILON_M &&
      Math.abs(pos.z - lastZ) < POSE_EPSILON_M &&
      Math.abs(yaw - lastYaw) < POSE_EPSILON_DEG &&
      Math.abs(camYaw - lastCamYaw) < POSE_EPSILON_DEG
    if (still) return
    lastX = pos.x
    lastZ = pos.z
    lastYaw = yaw
    lastCamYaw = camYaw
    ctx.send({ kind: 'playerPose', x: pos.x, z: pos.z, yaw, camYaw })
  })

  // Scene-title lookup state, shared with the realm poll below: crossing into another realm
  // replaces every scene without necessarily moving the player off the parcel (a World spawns
  // at 0,0), so the realm poll invalidates this to force a re-resolve.
  let publishedParcel = ''
  let pendingParcel = ''
  let attempts = 0

  // Realm kind → the minimap (Worlds have no map tiles). Poll ~2s, push on change.
  let realmAcc = 2
  let lastRealm = ''
  ctx.push((dt) => {
    realmAcc += dt
    if (realmAcc < 2) return
    realmAcc = 0
    getRealm({})
      .then(({ realmInfo }) => {
        const baseUrl = realmInfo?.baseUrl ?? ''
        const realmName = realmInfo?.realmName ?? ''
        // Key on both, so a change in either field re-publishes.
        const key = `${baseUrl}|${realmName}`
        if (key === lastRealm) return
        lastRealm = key
        const isWorld = realmIsWorld(baseUrl, realmName)
        // Only on change, so this is a handful of lines per session. Worth it: when the
        // minimap misreads a World the symptom (Genesis City markers on a world map) gives no
        // hint which of the two fields didn't match.
        console.log(`[world] realm baseUrl=${baseUrl} name=${realmName} isWorld=${String(isWorld)}`)
        ctx.send({ kind: 'realmInfo', realm: realmName || baseUrl, isWorld })
        // Every scene is replaced, but the player can land on the parcel they were already on
        // (a World spawns at 0,0), so the parcel guard alone would keep the old title forever.
        publishedParcel = ''
        pendingParcel = ''
        attempts = 0
      })
      .catch(() => undefined)
  })

  // Current scene title → the minimap header. Resolved by parcel from liveSceneInfo (the same
  // lookup `/reload` uses, isSuper filtered so it never reports the bridge). The sceneLoading
  // stream can't answer this: it describes the entry overlay, so it is empty once the overlay
  // clears and never changes as the player walks from one scene into the next.
  ctx.push(
    throttleByDt(
      SCENE_POLL_INTERVAL,
      singleFlight(async () => {
        const pos = getPlayer()?.position
        if (pos == null) return
        const px = Math.floor(pos.x / 16)
        const py = Math.floor(pos.z / 16)
        const key = `${px},${py}`
        if (key === publishedParcel) return
        if (key !== pendingParcel) {
          pendingParcel = key
          attempts = 0
        }
        attempts++
        const scenes = await BevyApi.liveSceneInfo().catch(() => null)
        if (scenes == null) return // transient RPC failure — the next tick retries
        const current = scenes.find((s) => !s.isSuper && s.parcels.some((p) => p.x === px && p.y === py))
        // A scene the player just walked (or teleported) into isn't in the live list until it
        // registers, so an immediate miss means "not yet", not "nothing here" — keep trying for
        // ~1s before settling on empty. bevy-ui-scene's widget did the same (20 tries × 100 ms).
        // While retrying the header keeps the previous title, so this budget is also how long a
        // stale name can survive after stepping onto an undeployed parcel.
        if (current == null && attempts < SCENE_LOOKUP_ATTEMPTS) return
        publishedParcel = key
        ctx.send({ kind: 'sceneInfo', title: current?.title ?? '' })
      })
    )
  )

  // Mic state → React mic toggle. Poll ~1s, push on change.
  let acc = 1
  let lastKey = ''
  ctx.push((dt) => {
    acc += dt
    if (acc < 1) return
    acc = 0
    BevyApi.getMicState()
      .then((s) => {
        const key = `${String(s.enabled)}|${String(s.available)}`
        if (key === lastKey) return
        lastKey = key
        ctx.send({ kind: 'mic', enabled: s.enabled, available: s.available })
      })
      .catch(() => undefined)
  })
}
