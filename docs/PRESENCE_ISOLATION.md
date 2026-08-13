# Per-scene player-presence isolation (orchestrated multi-tenant server)

One orchestrated engine process hosts scenes from different worlds, each with its own
LiveKit room (`SceneRoom(hash)` transport). Player presence, however, was process-global:
every room's inbound state funneled into one `GlobalCrdtState` store, and every scene read
all of it. A scene in world A could observe world B's players — address, position, name,
wearables, connect/disconnect timing — and a client in room B could inject MessageBus
messages into scene A by forging A's hash as `scene_id`.

The desktop client must be unaffected: there, all scenes share the realm transport and a
shared "nearby players" view is correct.

## Gate

`comms::PartitionPresenceByScene` (resource, default off). Set `true` only by
`src/bin/headless.rs` in orchestrated mode. When off, every path below is byte-for-byte
today's behavior.

## Mechanism

**CRDT store** (`crates/comms/src/global_crdt.rs`): `GlobalCrdtState` gains per-scene-hash
`RoomCrdt` slices (own `CrdtStore` + broadcast sender); the `SceneEntityId` allocator and
`Address→Entity` lookup stay global, so a player keeps the same id in every room they're
in. `update_crdt` takes `room: Option<&str>` — `None` is the global (client) path.
`process_transport_updates` resolves each update's arrival transport to its `SceneRoom`
hash and writes identity/transform into that slice only, recording the membership in
`player_rooms` so despawn and profile fan-out (`update_crdt_player`) hit exactly those
rooms even after the transport is gone. Scenes subscribe via `subscribe_room(hash)` when
partitioned (`load_scene_javascript`), receiving only their own room's players; time/fov
broadcasts fan out to all senders.

**RPC queries** (`crates/restricted_actions/src/lib.rs`): `getConnectedPlayers`,
`getPlayersInScene`, `getUserData`, `onPlayerConnected`, `onPlayerDisconnected` filter
through the `ScenePresence` param: visible iff the player is connected through the calling
scene's room transport (`ServerSceneRooms`). `RpcCall::GetConnectedPlayers` /
`SubscribePlayerConnected` / `SubscribePlayerDisconnected` carry the calling `scene` for
this. A scene with no room yet sees nobody (fail closed).

**MessageBus**: an inbound scene message is dropped unless its declared `scene_id` matches
the hash of the room it arrived on.

Known limitation: `FOREIGN_PLAYER_RANGE` remains one process-wide pool (~401 concurrent
players across all tenants); raising it is an independent follow-up.

## Validation

Two-tenant conformance harness (multiplayer-server): one engine, two scenes from different
worlds, one synthetic client per room. Assert (1) `getConnectedPlayers()` returns only the
own-room client; (2) `onPlayerConnected` fires only for it; (3) no foreign-room avatar
entities in the CRDT; (4) a message naming the co-tenant's hash never reaches its bus. All
four fail on `main`, pass with this change.
