use std::io::Result;
fn gen_sdk_components() -> Result<()> {
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
        "gltf_node",
        "gltf_node_state",
        "animator",
        "pointer_events",
        "pointer_events_result",
        "avatar_base",
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
        "ui_dropdown",
        "ui_dropdown_result",
        "ui_scroll_result",
        "ui_canvas",
        "text_shape",
        "pointer_lock",
        "camera_mode",
        "camera_mode_area",
        "audio_source",
        "video_player",
        "audio_stream",
        "video_event",
        "visibility_component",
        "avatar_modifier_area",
        "nft_shape",
        "tween",
        "tween_state",
        "light",
        "global_light",
        "spotlight",
        "texture_camera",
        "camera_layer",
        "camera_layers",
        "primary_pointer_info",
        "realm_info",
        "virtual_camera",
        "main_camera",
    ];

    let mut sources = components
        .iter()
        .map(|component| format!("src/proto/decentraland/sdk/components/{component}.proto"))
        .collect::<Vec<_>>();

    sources.push("src/proto/decentraland/kernel/comms/rfc5/ws_comms.proto".into());
    sources.push("src/proto/decentraland/kernel/comms/rfc4/comms.proto".into());
    sources.push("src/proto/decentraland/kernel/comms/v3/archipelago.proto".into());
    sources.push("src/proto/decentraland/social/friendships/friendships.proto".into());

    let serde_components = [
        "Vector2",
        "Color3",
        "PBRealmInfo",
        "PBAvatarBase",
        "PBAvatarEquippedData",
        "InputAction",
    ];

    let mut config = prost_build::Config::new();
    for component in serde_components {
        config.type_attribute(
            component,
            "#[derive(serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"camelCase\")]",
        );
    }

    config.compile_protos(&sources, &["src/proto/"])?;

    for source in sources {
        println!("cargo:rerun-if-changed={source}");
    }

    Ok(())
}

fn gen_social_service() -> Result<()> {
    let mut conf = prost_build::Config::new();
    conf.service_generator(Box::new(dcl_rpc::codegen::RPCServiceGenerator::new()));
    conf.type_attribute("*", "#[derive(Debug)]");
    conf.compile_protos(
        &["src/proto/decentraland/social/friendships/friendships.proto"],
        &["src/proto"],
    )?;
    Ok(())
}

fn main() -> Result<()> {
    gen_sdk_components()?;
    gen_social_service()?;
    Ok(())
}
