# @dcl-regenesislabs/bevy-headless-server

Decentraland authoritative scene server, powered by the [bevy explorer](https://github.com/decentraland/bevy-explorer)
engine. A drop-in replacement for `@dcl/hammurabi-server`: same CLI contract, native binary
instead of Node + Babylon.

```bash
npx @dcl-regenesislabs/bevy-headless-server --realm http://localhost:8000
```

The Decentraland SDK spawns this automatically for scenes with `authoritativeMultiplayer`
enabled; you rarely need to run it by hand.

## Options

| Flag | Meaning |
| --- | --- |
| `--realm <url>` | Realm to serve. Required. |
| `--position <x,y>` | Parcel to load. Defaults to `0,0`. |
| `--production` | Production mode; disables preview-only behaviour. |
| `--tick-hz <n>` | Scene tick rate. Defaults to 30. |
| `--timeout <secs>` | Exit cleanly after N seconds. |
| `--viewer` | Open a window rendering the server's own world. Preview, single-scene only. |

`--scene-id`, `--private-key` and `--env` are accepted for hammurabi compatibility and ignored.

## Debug viewer

`--viewer` (or a truthy `DCL_SERVER_VIEWER`) opens a window showing what the server sees:
the scene, remote players as capsules at the positions the server trusts, a clickable
player list that focuses and orbit-follows a selection, and a free-fly camera.

`sdk-commands start` forwards the whole environment to the server it spawns, so no SDK
flag is needed:

```bash
DCL_SERVER_VIEWER=1 npm start
```

Controls: `WASD`/`Q`/`E` fly (`shift` faster, `ctrl` slower), right-drag to look, wheel to
zoom, `Tab` to cycle players, `F` to frame everyone, `Esc` back to the free camera.

It is a debug mode, not a second server: the process is still the authoritative server and
every server-mode rule still applies (no avatar broadcast, no Pulse, server gatekeeper).
But the render plugins it switches on are the client's, so a viewer run is not
production-identical and its memory and CPU profile are not the ones measured for
headless. Refused with `--orchestrated` (scenes overlap in world space) and without
`--preview` — the launcher always passes `--preview` for standalone serving, so this only
constrains hand-rolled engine invocations.

## Orchestrated mode (multiplayer-server)

`--orchestrated` starts the engine in multi-scene worker mode: no realm needed, scenes are
added and removed over stdin (JSON lines) with pre-minted comms adapters, and control
events come back on stdout with the `@bevy-ctl ` prefix.

Orchestrators that manage the process themselves can skip the CLI and resolve the engine
path programmatically:

```js
const { resolveBinary } = require('@dcl-regenesislabs/bevy-headless-server')
spawn(resolveBinary(), ['--orchestrated'], { stdio: ['pipe', 'pipe', 'inherit'] })
```

## How the binary is delivered

The engine ships as four platform packages (`darwin-arm64`, `darwin-x64`, `linux-x64`,
`win32-x64`) listed under `optionalDependencies`; your package manager installs only the one
matching your machine. Each contains two files that **must stay in the same directory** —
the engine execs its scene-runtime sidecar from its own location.

Set `DCL_BEVY_SERVER_PATH` to an absolute path to use a pre-installed engine instead
(useful for Electron hosts that bundle their own copy).

Exit code `78` means the engine is permanently unavailable here — unsupported platform,
missing binary, or bad arguments — so a caller can fall back to another implementation
instead of retrying.

## Linux runtime dependencies

The engine links the graphics/audio stack even when running headless:

```bash
sudo apt install libasound2 libudev1 libgl1 libx11-6 libxext6
```
