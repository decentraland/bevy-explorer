use std::io::Result;
fn main() -> Result<()> {
    let components = [
        "engine_info",
        "billboard",
        "raycast",
        "raycast_result",
        "mesh_renderer",
        "mesh_collider",
        "material",
        "gltf_container",
        "gltf_container_loading_state",
        "animator",
        "pointer_events",
        "pointer_events_result",
    ];

    let mut sources = components
        .iter()
        .map(|component| {
            format!("src/dcl_component/proto/decentraland/sdk/components/{component}.proto")
        })
        .collect::<Vec<_>>();

    sources.push("src/dcl_component/proto/decentraland/kernel/comms/rfc5/ws_comms.proto".into());

    prost_build::compile_protos(&sources, &["src/dcl_component/proto/"])?;
    Ok(())
}
