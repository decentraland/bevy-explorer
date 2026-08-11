# bevy-headless as the local preview authoritative server

Replaces the Node `@dcl/hammurabi-server` that `sdk-commands start` spawns for
authoritative-multiplayer scenes, with the Rust `headless` binary from this repo.

## What the SDK actually requires

`sdk-commands start` (auth-server branch) spawns the server unconditionally for the first
project in the workspace:

```
npx --yes @dcl/hammurabi-server@next --realm=http://localhost:<port>
```

`cwd` is the project directory, `stdio` is `inherit`, and the child is killed with SIGTERM
on shutdown. That is the entire contract — `--realm` is the only argument, and the CLI
never parses the child's output, so there is no readiness marker to reproduce.

The scene room both servers join is **not** the `ws-room` fixed adapter from `/about`.
It is a LiveKit room minted by the comms gatekeeper, keyed on the scene entity id, which
the preview server derives from the scene path:

```
scene id : b64-<base64(<sceneDir>-<hostname>)>
room     : preview-<scene id>
```

Both implementations resolve the same id from the same realm, so they mint the same room.
(Consequence worth knowing: two previews of the *same directory* on different ports share
one room and will fight over the synced entities.)

## Running it

```bash
# what the launcher does under the hood
headless --realm http://localhost:8000 --preview --server-mode --location 0,0
```

`--server-mode` makes `isServer()` true for scene code; `--preview` selects the local
gatekeeper and disables the failed-asset backoff. The npm launcher translates hammurabi's
`--realm=<url>` form into exactly this.

## Testing it locally

Build the binaries and assemble the platform package once:

```bash
cargo build --release -p dcl_deno_ipc
cargo build --release --bin headless --no-default-features --features headless,livekit
node deploy/headless/build-platform-package.js --platform darwin-arm64 --version 0.1.0 \
  --engine target/release/headless --sidecar target/release/dcl_deno_ipc --out deploy/headless/dist
```

**The command on its own** — `npm link` the platform package into the launcher, then the
launcher globally:

```bash
(cd deploy/headless/dist/bevy-headless-server-darwin-arm64 && npm link)
(cd deploy/headless/launcher && npm link @dcl-regenesislabs/bevy-headless-server-darwin-arm64 && npm link)

bevy-headless-server --realm=http://localhost:8000
```

**The full SDK spawn path**, without publishing anything. `npx` takes a directory or a
tarball as well as a registry spec, so `DCL_SERVER_PACKAGE` pointed at `deploy/headless/launcher`
makes `sdk-commands start` install and run the local build exactly as it will run the
published one:

```bash
cd <your scene>
DCL_SERVER_ENGINE=bevy \
DCL_SERVER_PACKAGE=/path/to/bevy-explorer/deploy/headless/launcher \
  npm start
```

Look for `[Game] Running as SERVER` / `[Server] Ready` in the preview output, and confirm
the child is the engine rather than node with `pgrep -fl "headless --realm"`.

To verify a client syncs without needing a GPU, run a second engine as a plain client and
watch for `[Game] Connected to server`:

```bash
headless --realm http://localhost:8000 --preview --location 0,0
```

## Validation (2026-07-31, towerofmadness)

Verified against a real `sdk-commands start` preview with an identical headless client
attached, once with each server. Both arms produced the same signals:

| Signal | Source |
| --- | --- |
| `[Game] Running as SERVER`, `[Server] Ready` | server stdout |
| `[Server] Player joined: 0x…` | server saw the client |
| `[TimeSync] Server received sync request from 0x…` | message-bus round trip |
| `[Game] Connected to server` | client received CRDT state from the `authoritative-server` peer |
| `[Debug] TriggerEnd found at …` | server-created synced entity reached the client |
| `[TimeSync] Synchronized, offset: -12ms` | full time-sync handshake |
| `[Server][Storage] Loaded global leaderboard: 4 entries` | scene storage via the preview `/values` endpoints |

The timer — the acceptance criterion — was proven by letting a whole round cycle run on
bevy: the round ended at **t=420.16s** (the scene's 420s timer), moved through
`ENDING` (3.2s) and `BREAK` (10.1s), started a new round, and the client logged
`[Cinematic] New round detected`. `RoundState` CRDT writes carry the expected
`remainingAtSpeedChange` / `lastSpeedChangeTime` anchor the client extrapolates from.

### Known cosmetic noise

The engine logs `Could not find an asset loader` for its embedded fonts and
`failed to process gltf … /.` for asset-pack items and cleared `GltfContainer`s. These
appear **identically when the same binary runs as a client**, so they are pre-existing
asset-resolution noise rather than a server-mode regression. The scene's own tower chunks
load without error. Worth a follow-up: the failing paths have a spurious `.` inserted
before the filename (`…/trigger_area/.trigger_area.glb`).

## Cost vs hammurabi

Measured on the same machine, same scene, one connected client, both sampled for 3 minutes
during the second round so the scene lifecycle matches. CPU is the derivative of cumulative
CPU time (macOS `ps %cpu` is a lifetime average and cannot be used directly).

| | hammurabi (node) | bevy-headless | delta |
| --- | --- | --- | --- |
| CPU, steady state | 6.7% of one core | 16.5% of one core | **2.5× more** |
| RSS, steady state | 308 MB (286–339) | 175 MB | **−43%** |
| processes | 1 | 2 (engine 96 MB + sidecar 79 MB) | +1 |
| scene tick rate | — | 29.97 Hz (30 Hz target) | — |
| download, cold install | 96 MB (62 packages) | 71 MB (2 packages) | **−26%** |
| on disk after install | 236 MB | 154 MB | **−35%** |
| time to `[Server] Ready` | 1.5–3.0 s | 0.7 s | **2–4× faster** |

Method: `treesample.py` samples the whole process tree every 5 s via `ps`; `analyze.py`
takes the CPU derivative over the window and drops the first 30% of samples. Both arms ran
the same scene against the same preview server with one identical headless client attached,
and both were sampled for 180 s starting 460 s after boot — i.e. during the second round,
after the first tower had been built, destroyed and rebuilt.

The bevy RSS and tick figures were re-measured on 2026-08-07 after the headless build stopped
decoding glTF textures and stopped constructing render-only scene plugins; the original
reading was 234 MB. **These two rows are not a like-for-like swap**: they come from
`bench/sample.py` with no client attached, sampled from boot, whereas the rest of the table
was taken with one connected client during the second tower round using a `treesample.py`
that is no longer in the repo. A connected client only adds work, so 175 MB is a floor for
the original conditions rather than a contradiction of them. The CPU, download, on-disk and
startup rows are unchanged from 2026-07-31 and were not re-measured.

Caveat on RSS: measured with `ps rss`, which counts shared pages per process, so the
two-process bevy figure is if anything slightly pessimistic. Single-sample RSS is very noisy
for both (V8's GC sawtooth for hammurabi, asset load/free for bevy) — earlier uncontrolled
runs ranged 140–465 MB for hammurabi and 218–550 MB for bevy, which is why the lifecycle is
pinned above.

**Read this honestly:** for a single scene the bevy server costs more CPU than hammurabi.
Its advantage is consolidation — one engine hosting many scenes, which is the deployment
model on the server side — plus a smaller and simpler install. For a scene developer
running one scene locally, the win is install size, startup and memory, not CPU.

## Packaging

`deploy/headless/` holds the publishable layout, built by `.github/workflows/publish-headless.yml`:

- `@dcl-regenesislabs/bevy-headless-server` — a ~10 KB JS launcher. Translates hammurabi's CLI, resolves
  the platform package, forwards signals, and exits `78` when the engine can't run here.
- `@dcl-regenesislabs/bevy-headless-server-{darwin-arm64,darwin-x64,linux-x64,win32-x64}` — the binary
  pair, selected automatically through `optionalDependencies` + `os`/`cpu` fields
  (the pattern esbuild uses, which `sdk-commands` already depends on).

The same package serves the multiplayer-server: the CLI passes `--orchestrated` through
(stdin control protocol, no realm), and the package's main export exposes
`resolveBinary()` for orchestrators that spawn the engine directly — replacing the
Dockerfile's build-from-source stage with an `npm install`.

Verified locally on darwin-arm64: both packages `npm pack`ed, installed into a clean
project, and `./node_modules/.bin/bevy-headless-server --realm=http://localhost:8100`
booted the scene as SERVER and exited 0.

The SDK side is one file on the js-sdk-toolchain `auth-server` branch
(`commands/start/hammurabi-server.ts`): a `DCL_SERVER_ENGINE=bevy|hammurabi` switch that
picks the package name, a `DCL_SERVER_PACKAGE` override so a local build can be spawned
without publishing, and a `close` handler that falls back to hammurabi on exit 78.
Default stays `hammurabi` until the bevy packages are published.

## Known gap: `--bevy-web` cannot reach a local realm

`sdk-commands start --bevy-web` opens the hosted client at
`https://decentraland.zone/bevy-web/?preview=true&realm=http://localhost:<port>`. That
client fails with *"World not found — the world isn't reachable right now"* for both
`localhost` and the LAN IP: an https page cannot fetch an http realm. It fails at the
client→realm hop, before any contact with the authoritative server, so it is unrelated to
which server implementation runs. Use the native explorer, or a headless client
(`headless --realm <url> --preview`), to exercise a local preview end to end.
