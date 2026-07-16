# dev entry points. `just --list` for a summary.
# node recipes assume node 20 on PATH (nvm users: the interactive shell's PATH is inherited).

# build the wasm engine into deploy/web/engine/pkg, then serve the react-web page (which hosts
# the engine + live bridge-scene preview) and open a browser at the vite dev server.
wasm:
    wasm-pack build --target web --out-dir ./deploy/web/engine/pkg --no-default-features --features="livekit,social"
    rm -f ./deploy/web/engine/pkg/.gitignore
    WASM_SIZE=$(wc -c < ./deploy/web/engine/pkg/webgpu_build_bg.wasm) && echo "{\"wasmSize\":${WASM_SIZE}}" > ./deploy/web/engine/pkg/manifest.json
    cd react-web && npm run dev -- --open

# regenerate the TypeScript bindings for the ~system/BevyExplorerApi boundary from the Rust
# structs in crates/system_api_types, into react-web/src/engine/generated (+ a barrel index.ts).
ts-bindings:
    {{justfile_directory()}}/scripts/gen-ts-bindings.sh

# bundle the react HUD page + bridge scene into assets/ (the files native runs from).
# ts-bindings first so the TS the page + bridge scene import is regenerated from the Rust structs.
bundle-native: ts-bindings
    cd react-web && npm install
    cd react-web/bridge-scene && npm install
    cd react-web && npm run bundle:native
    mkdir -p target && touch target/.bundle-native-stamp

# bundle only if react-web sources changed since the last successful bundle
_bundle-native-if-stale:
    #!/usr/bin/env sh
    set -eu
    stamp=target/.bundle-native-stamp
    if [ -f "$stamp" ] && [ -f assets/react-hud/index.html ] \
       && [ -f assets/bridge-scene/BevyExplorerUI/about ] \
       && [ -z "$(find react-web -name node_modules -prune -o -type f -newer "$stamp" -print | head -n 1)" ]; then
        echo "react-web unchanged; skipping bundle (just bundle-native to force)"
    else
        just bundle-native
    fi

# build + run the native app (debug) with the CEF react HUD. extra args pass through, e.g.
#   just native-debug --server https://realm-provider.decentraland.org/main
native-debug *ARGS: _bundle-native-if-stale
    cargo build --package dcl_deno_ipc
    cargo build --bin decentra-bevy-cef
    cargo run --bin decentra-bevy -- {{ARGS}}

# build + run the native app (release) with the CEF react HUD
native-release *ARGS: _bundle-native-if-stale
    cargo build --release --package dcl_deno_ipc
    cargo build --release --bin decentra-bevy-cef
    cargo run --release --bin decentra-bevy -- {{ARGS}}

# one-time per machine: fetch the CEF framework the dev fallback loads from
setup-cef:
    cargo install export-cef-dir --version "139.8.0+139.0.40"
    export-cef-dir --force $HOME/.local/share/cef
