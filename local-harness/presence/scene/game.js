// Presence-isolation assertion scene (raw SDK7 ops, no @dcl/sdk toolchain).
// Logs greppable HARNESS| lines the run.sh matrix parses. All player-visibility
// facts (getConnectedPlayers, connect/disconnect events, PLAYER_IDENTITY_DATA
// entities in the scene's own CRDT view, comms bus messages) are surfaced here so
// the harness can prove that a scene only ever sees its own room's clients.

const engine = require("~system/EngineApi");
const players = require("~system/Players");

// getConnectedPlayers is an async RPC. It must NOT be awaited inline: scene-issued
// RpcCalls only reach the engine on the next crdtSendToRenderer, so awaiting the reply
// inside onUpdate parks the loop before that flush and self-deadlocks. We fire it and
// store the result, letting later onUpdate ticks keep pumping the flush; the reply lands
// a frame or two later and is logged then.
const WANT_CONNECTED_PLAYERS = true;

const PLAYER_IDENTITY_DATA = 1089;
const PUT_COMPONENT = 1;
const DELETE_COMPONENT = 2;
const DELETE_ENTITY = 3;
const APPEND_VALUE = 4;

// entity number -> address, built from the CRDT stream the renderer feeds this scene
const identityByEntity = {};

function log(kind, payload) {
  console.log("HARNESS|" + kind + "|" + JSON.stringify(payload));
}

// PbPlayerIdentityData: field 1 (address) is a length-delimited string (tag 0x0a).
function parseIdentityAddress(bytes) {
  if (bytes.length < 2 || bytes[0] !== 0x0a) return null;
  const len = bytes[1];
  return new TextDecoder().decode(bytes.slice(2, 2 + len));
}

// Walk one renderer->scene buffer, which may hold several concatenated CRDT
// messages, and fold PLAYER_IDENTITY_DATA puts/deletes into identityByEntity.
function ingestCrdt(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let off = 0;
  while (off + 8 <= bytes.length) {
    const len = view.getUint32(off, true);
    if (len < 8 || off + len > bytes.length) break;
    const type = view.getUint32(off + 4, true);
    if (type === PUT_COMPONENT || type === DELETE_COMPONENT) {
      const entityNum = view.getUint16(off + 8, true);
      const componentId = view.getUint32(off + 12, true);
      if (componentId === PLAYER_IDENTITY_DATA) {
        if (type === PUT_COMPONENT) {
          const contentLen = view.getUint32(off + 20, true);
          const content = bytes.slice(off + 24, off + 24 + contentLen);
          const address = parseIdentityAddress(content);
          if (address) {
            identityByEntity[entityNum] = address;
            log("identity-put", { entity: entityNum, address: address });
          }
        } else {
          const gone = identityByEntity[entityNum];
          delete identityByEntity[entityNum];
          log("identity-delete", { entity: entityNum, address: gone || null });
        }
      }
    } else if (type === DELETE_ENTITY) {
      const entityNum = view.getUint16(off + 8, true);
      if (identityByEntity[entityNum]) {
        log("identity-delete", { entity: entityNum, address: identityByEntity[entityNum] });
        delete identityByEntity[entityNum];
      }
    }
    off += len;
  }
}

async function drainCrdt() {
  // send an empty batch and read back the renderer's CRDT for this frame; the first
  // call returns the initial snapshot, later calls return deltas. This pairs one send
  // with one recv — never a bare recv, which can park onStart on the renderer channel.
  const res = await engine.crdtSendToRenderer({ data: new Uint8Array(0) });
  for (const item of res.data) ingestCrdt(item);
}

let ticks = 0;

// getConnectedPlayers result plumbing: `connectedInFlight` guards against stacking
// calls, and each resolved reply is logged from the promise callback (not awaited).
let connectedInFlight = false;

function pollConnectedPlayers(tick) {
  if (connectedInFlight) return;
  connectedInFlight = true;
  players
    .getConnectedPlayers()
    .then((connected) => {
      log("connected-players", {
        tick: tick,
        players: connected.players.map((p) => p.userId),
      });
    })
    .catch((e) => log("connected-players-error", { tick: tick, msg: String(e) }))
    .finally(() => {
      connectedInFlight = false;
    });
}

module.exports.onStart = async function () {
  log("scene-start", {});
  // SDK6-style event subscriptions; polled each frame via sendBatch. No CRDT recv here:
  // the initial snapshot is drained by the first onUpdate.
  await engine.subscribe({ eventId: "playerConnected" });
  await engine.subscribe({ eventId: "playerDisconnected" });
  await engine.subscribe({ eventId: "comms" });
  log("scene-subscribed", {});
};

function rosterAddrs() {
  return Object.keys(identityByEntity).map((e) => identityByEntity[e]);
}

module.exports.onUpdate = async function (_dt) {
  ticks += 1;
  try {
    await drainCrdt();

    const batch = await engine.sendBatch();
    for (const ev of batch.events) {
      if (ev.generic) {
        log("event", { id: ev.generic.eventId, data: ev.generic.eventData });
      }
    }

    // Report the scene's authoritative player view (PLAYER_IDENTITY_DATA entities in
    // its own CRDT) once a second. This is the isolation signal: a scene must only ever
    // carry its own room's clients. Emitted separately (getConnectedPlayers, an async RPC)
    // once things are stable — the CRDT roster is the source of truth.
    if (ticks % 30 === 0) {
      log("identity-roster", { tick: ticks, players: rosterAddrs() });
      // getConnectedPlayers is fired, NOT awaited (see pollConnectedPlayers): awaiting it
      // here would stall this frame's next crdtSendToRenderer flush and deadlock the RPC.
      // The CRDT roster above is the authoritative isolation signal; this cross-checks it.
      if (WANT_CONNECTED_PLAYERS) {
        pollConnectedPlayers(ticks);
      }
    }
  } catch (e) {
    log("error", { tick: ticks, msg: String(e), stack: e && e.stack });
  }
};
