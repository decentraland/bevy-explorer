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

// Is the player already in the realm a place lives in? A world target must be that exact world;
// a Genesis City target is satisfied by any non-World realm (there is one Genesis, and its base
// url varies by deployment, so the realm kind is the honest comparison).
function inRealm(baseUrl: string, realmName: string, target: string): boolean {
  return target.endsWith('.eth')
    ? realmName.toLowerCase() === target.toLowerCase()
    : !realmIsWorld(baseUrl, realmName)
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

  const teleportToParcel = (x: number, y: number): void => {
    teleportTo({ worldCoordinates: { x, y } }).catch((e: unknown) => {
      console.error('[world] teleport failed', e)
    })
  }

  // A teleport held until its realm change lands (see the teleport handler). `waited` is seconds
  // accumulated by the realm poll, so a realm that never arrives drops it instead of teleporting
  // into whatever realm the player ends up in.
  let pendingTeleport: { realm: string; x: number; y: number; waited: number } | null = null
  const REALM_ARRIVAL_TIMEOUT = 60

  // Teleport to a parcel. Without a realm the coordinates are relative to wherever the player
  // already is (a chat location link, a photo's capture spot) and nothing changes realm.
  //
  // With one, the parcel belongs to that realm, and getting there is part of the trip. It is
  // resolved here rather than in React because this is where the current realm is known
  // first-hand: React only sees the 2s realmInfo poll and would read a stale realm right after a
  // change. The switch is skipped when the player is already there — a changeRealm to the realm
  // you are in is a full scene purge + reconnect, not a no-op.
  //
  // The parcel CANNOT be sent on changeRealm's promise. That promise resolves as soon as the engine
  // ACCEPTS the change (restricted_actions/src/lib.rs: it queues a ChangeRealmEvent and answers Ok);
  // the about fetch, scene purge and pointer re-resolve all happen over the following frames. A
  // teleport sent there addresses the parcel grid of the realm we are LEAVING — you get one
  // out-of-world round trip inside the old realm, then a second one when the realm actually lands.
  // So the parcel waits for getRealm to report the target realm; the poll below completes it.
  ctx.on('teleport', (msg) => {
    const realm = msg.realm
    if (realm == null) {
      teleportToParcel(msg.x, msg.y)
      return
    }
    getRealm({})
      .then(async ({ realmInfo }) => {
        if (inRealm(realmInfo?.baseUrl ?? '', realmInfo?.realmName ?? '', realm)) {
          teleportToParcel(msg.x, msg.y)
          return
        }
        pendingTeleport = { realm, x: msg.x, y: msg.y, waited: 0 }
        await changeRealm({ realm }).catch((e: unknown) => {
          console.error('[world] changeRealm failed', e)
          pendingTeleport = null
        })
      })
      .catch((e: unknown) => {
        console.error('[world] teleport failed', e)
        pendingTeleport = null
      })
  })

  // Travel to a world/realm (e.g. boedo.dcl.eth) with no destination parcel. The engine
  // auto-grants ChangeRealm for our super-user scene, so the React HUD owns the confirmation.
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
      .then((scenes) => {
        const current = scenes.find((s) => s.isSuper !== true && (s.parcels ?? []).some((p) => p.x === px && p.y === py))
        if (current == null) {
          pushSystem(ctx, 'Could not find the current scene to reload.')
          return
        }
        return op('reload', [current.hash]).then(() => pushSystem(ctx, `Reloading ${current.title || current.hash}…`))
      })
      .catch((e: unknown) => {
        console.error('[world] reload failed', e)
        pushSystem(ctx, 'Reload failed.')
      })
  })

  // `/commands` — surface the engine console's own command list. Run its `help`; if `help` isn't a
  // registered command the engine rejects with "Recognized commands: [...]" — exactly the list we
  // want — so relay either the successful output or the rejection text.
  ctx.on('consoleCommand', (msg) => {
    const op = BevyApi.consoleCommand
    if (op == null) {
      pushSystem(ctx, 'Engine console is not available.')
      return
    }
    op(msg.command, msg.args ?? [])
      .then((out) => pushSystem(ctx, out.trim() || `(no output for ${msg.command})`))
      .catch((e: unknown) => pushSystem(ctx, e instanceof Error ? e.message : String(e)))
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

  // Realm kind → the minimap (Worlds have no map tiles). Poll ~2s, push on change. While a teleport
  // is waiting for its realm, poll ~10×/s instead: that wait is a player standing in front of the
  // loading screen, and up to 2s of it would be spent looking at the wrong parcel.
  let realmAcc = 2
  let lastRealm = ''
  ctx.push((dt) => {
    realmAcc += dt
    if (pendingTeleport != null) {
      pendingTeleport.waited += dt
      if (pendingTeleport.waited > REALM_ARRIVAL_TIMEOUT) {
        console.error(`[world] realm ${pendingTeleport.realm} never arrived; dropping the teleport`)
        pendingTeleport = null
      }
    }
    if (realmAcc < (pendingTeleport == null ? 2 : 0.1)) return
    realmAcc = 0
    getRealm({})
      .then(({ realmInfo }) => {
        const baseUrl = realmInfo?.baseUrl ?? ''
        const realmName = realmInfo?.realmName ?? ''
        // The realm we were held for is live: finish the trip. Done before the change check below
        // so it doesn't depend on the minimap's dedupe key.
        if (pendingTeleport != null && inRealm(baseUrl, realmName, pendingTeleport.realm)) {
          const target = pendingTeleport
          pendingTeleport = null
          teleportToParcel(target.x, target.y)
        }
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
