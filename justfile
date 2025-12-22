set shell := ["nu", "-c"]

run $RUST_LOG="warn,comms::livekit::room=debug,comms::livekit::participant=debug,comms::livekit::track=debug,comms::livekit::mic=debug":
    cargo build -p dcl_deno_ipc
    cargo run --jobs 8 -- --ui https://dclexplorer.github.io/bevy-ui-scene/BevyUiScene --scene_log_to_console --server boedo.dcl.eth --no-perms --no-chat --no-profile --no-nametags
    
test:
    cargo test --all --jobs 4

bevy:
    bevy run --bin decentra-bevy web --open

wasm:
    wasm-pack build --dev --target web --out-dir ./deploy/web/pkg --no-default-features --features="livekit"
    npx serve deploy/web

link:
    ln -s ./crates/dcl/src/js/modules/ ./deploy/web/modules

clippy-native:
    cargo clippy -p dcl_deno_ipc
    cargo clippy --no-default-features --features="ffmpeg inspect social"
    cargo clippy

clippy-wasm:
    cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features="livekit"

diff:
    cargo tree --features "ffmpeg livekit" -e features -p decentra-bevy | parse -r ' (?<crate>\w*) feature \"(?<feature>\w*)\"' | sort | uniq-by crate feature | to csv | save -f decentra-bevy.deps
    cargo tree --features "ffmpeg comms/livekit" -e features -p av | parse -r ' (?<crate>\w*) feature \"(?<feature>\w*)\"' | sort | uniq-by crate feature | to csv | save -f av_livekit_ffmpeg.deps
    diff -y --suppress-common-lines decentra-bevy.deps av_livekit_ffmpeg.deps out> diff.txt
