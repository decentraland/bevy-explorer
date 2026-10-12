// The scene runtime's memory holds one scene's state, so cap it at the native sidecar's per-scene
// heap limit (512 MiB) rather than the workspace's 4 GiB. The scene's JS heap is separate.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg=--max-memory=536870912");
    }
}
