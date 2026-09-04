# Per-scene player-presence isolation (orchestrated multi-tenant server)

One orchestrated engine process hosts scenes from different worlds, each with its own
LiveKit room. Player presence used to be process-global: every room's inbound state
funneled into one `GlobalCrdtState` resource, and every scene read all of it. A scene in
world A could observe world B's players — address, position, name, wearables,
connect/disconnect timing — and a client in room B could inject MessageBus messages into
scene A by forging A's hash as `scene_id`.

## Mechanism: one crdt context per room

`GlobalCrdtState` is a **component, not a resource** — one instance is one complete
player-presence view: its own crdt store, its own transport-facing channel, its own
scene-facing broadcast, its own `Address→Entity` lookup, and its own player-id allocator.

- The **client** has exactly one, the *shared context*, spawned by `GlobalCrdtPlugin`.
  Every transport (realm, archipelago islands, client scene rooms) and every scene
  resolves to it, so the shared "nearby players" view is unchanged.
- An **orchestrated server** spawns one additional context per scene room
  (`drain_control_commands`), reused across adapter re-mints, and despawns it with the
  scene (`RemoveScene`); a context observer despawns its player entities with it.

There is no mode flag and no per-packet routing decision. A transport is bound to a
context once, at connect time (`AdapterManager::connect(adapter, context)` → the `Start*`
events → `Transport.context`), so a packet physically cannot reach another context's
store. Scenes resolve their context by hash (`CrdtContexts::for_scene_hash`): a room
context if one exists, else the shared context — which on a server has no transports
bound, so an (unexpected) roomless scene sees nobody.

## Player entities

`ForeignPlayer` entities belong to a context (`ForeignPlayer.context`). The same wallet
connected to two rooms is simply **two entities**, one per context, each with its own
`scene_id` allocated from its context's `PlayerIdAllocator` (over
`SceneEntityId::FOREIGN_PLAYER_RANGE`, so each room hosts the full 224-player range;
freed ids are re-issued with a bumped generation so scenes see a recycled id as a fresh
entity). Per-room connect/disconnect therefore falls out of ordinary entity lifecycle:
joining a second room creates a player entity in that context (identity write + profile
replication + `onPlayerConnected`); leaving one room despawns that room's entity
(crdt delete + `onPlayerDisconnected`) while the other room's entity lives on.

## RPC scoping

`getConnectedPlayers`, `getPlayersInScene`, `getUserData`, `onPlayerConnected`,
`onPlayerDisconnected` filter by context equality: a player is visible to a scene iff
`player.context == ScenePresence::context_of(scene)`. `RpcCall::GetConnectedPlayers` /
`SubscribePlayerConnected` / `SubscribePlayerDisconnected` carry the calling scene for
this. `getPlayersInScene` uses context membership for room-scoped scenes (positional
checks are meaningless on a multi-tenant server, where scenes host as portables) and
stays positional on the client.

## MessageBus

An inbound scene message arriving on a room context is dropped unless its declared
`scene_id` matches the context's room hash. Outbound scene messages were already
room-filtered server-side (`send_scene_messages`).

## The deno IPC sidecar

Native scene JS runs in the `dcl_deno_ipc` sidecar process, and the scene-facing
broadcast stream crosses that pipe. The wire protocol tags each update with its context:
`EngineToScene::GlobalUpdate(presence_context, update)`. The engine pump keeps one
receiver per live context; the sidecar keeps one broadcast channel per context id, which
each scene subscribes to at `NewScene` (`NewSceneInfo.presence_context`). Previously the
sidecar had a single process-wide channel fanning untagged updates to every scene —
correct when all subscriptions were clones of one stream, but a cross-tenant leak with
per-context streams.

New-scene continuity keeps the original swap semantics, per context: a scene's engine-side
receiver is created at its store-snapshot time, and on `NewScene` the pump swaps the
context's receiver for the incoming one — replaying every update since the snapshot after
the `NewScene` message, so the new scene misses nothing while old scenes idempotently
re-apply a few duplicated crdt messages. The previous receiver's backlog is flushed down
the pipe before the swap, so old scenes stay continuous (this also closes two windows the
old code had: a swapped-out backlog was silently dropped, and rapid successive scene
creations could gap the middle scene). A closed receiver (context despawned) is dropped;
the sidecar drops a channel when an update finds no subscribers.

## Known limitations

- Realm bounds (`MovementCompressed` dequantization) are applied to every context —
  per-world bounds for multi-tenant servers are a follow-up.
- Client-render-path systems that correlate avatars across scenes (colliders, pointer
  events, emotes) assume the single shared context; they are inert or absent on the
  headless server, as before.
