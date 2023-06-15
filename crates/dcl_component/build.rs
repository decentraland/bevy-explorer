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
        "avatar_customization",
        "avatar_emote_command",
        "avatar_equipped_data",
        "player_identity_data",
        "avatar_shape",
        "avatar_attach",
        "ui_transform",
        "ui_text",
        "ui_background",
        "ui_input",
        "ui_input_result",
        "ui_canvas_information",
    ];

    let mut sources = components
        .iter()
        .map(|component| format!("src/proto/decentraland/sdk/components/{component}.proto"))
        .collect::<Vec<_>>();

    sources.push("src/proto/decentraland/kernel/comms/rfc5/ws_comms.proto".into());
    sources.push("src/proto/decentraland/kernel/comms/rfc4/comms.proto".into());

    let serde_components = ["Color3"];

    let mut config = prost_build::Config::new();
    for component in serde_components {
        config.type_attribute(component, "#[derive(serde::Serialize, serde::Deserialize)]");
    }

    config.compile_protos(&sources, &["src/proto/"])?;

    for source in sources {
        println!("cargo:rerun-if-changed={source}");
    }

    Ok(())
}
