set shell := ["nu", "-c"]
export RUST_LOG := "warn,dcl_component=debug,tween=debug"

run:
    cargo build --jobs 4 -p dcl_deno_ipc
    cargo run --jobs 4 -- --preview --ui https://dclexplorer.github.io/bevy-ui-scene/BevyUiScene --scene_log_to_console --server http://localhost:8000 --no-perms --no-chat --no-profile --no-nametags

test:
    cargo build --jobs 4 -p dcl_deno_ipc --release
    cargo test --jobs 4 --all --release -- --test-threads 1

bevy:
    bevy run --bin decentra-bevy web --open

wasm:
    wasm-pack build --dev --target web --out-dir ./deploy/web/pkg --jobs 4 --no-default-features --features="livekit"
    npx serve deploy/web

link:
    ln -s ./crates/dcl/src/js/modules/ ./deploy/web/modules

clippy-native:
    cargo clippy --jobs 4 -p dcl_deno_ipc
    cargo clippy --jobs 4 --no-default-features --features="ffmpeg inspect social"
    #cargo clippy --features="av/av_player_debug"

clippy-wasm:
    cargo clippy --jobs 4 --lib --target wasm32-unknown-unknown --no-default-features
    cargo clippy --jobs 4 --lib --target wasm32-unknown-unknown --no-default-features --features="livekit"
    #cargo clippy --lib --target wasm32-unknown-unknown --no-default-features --features="livekit av/av_player_debug"

clippy-tween:
    cargo clippy --jobs 4 -p tween --no-default-features
    cargo clippy --jobs 4 -p tween --no-default-features --features="tween_debug"
    cargo clippy --jobs 4 -p tween --no-default-features --features="adr285"
    cargo clippy --jobs 4 -p tween --no-default-features --features="adr285 alt_rotate_continuous"
    cargo clippy --jobs 4 -p tween --no-default-features --features="adr285 tween_debug"
    cargo clippy --jobs 4 -p tween --no-default-features --features="adr285 alt_rotate_continuous tween_debug"

diff:
    cargo tree --features "ffmpeg livekit" -e features -p decentra-bevy | parse -r ' (?<crate>\w*) feature \"(?<feature>\w*)\"' | sort | uniq-by crate feature | to csv | save -f decentra-bevy.deps
    cargo tree --features "ffmpeg comms/livekit" -e features -p av | parse -r ' (?<crate>\w*) feature \"(?<feature>\w*)\"' | sort | uniq-by crate feature | to csv | save -f av_livekit_ffmpeg.deps
    diff -y --suppress-common-lines decentra-bevy.deps av_livekit_ffmpeg.deps out> diff.txt
