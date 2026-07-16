use std::io::Result;

mod build_schema;

fn gen_sdk_components() -> Result<()> {
    let components = [
        "asset_load",
        "asset_load_loading_state",
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
        "audio_event",
        "visibility_component",
        "avatar_modifier_area",
        "nft_shape",
        "tween",
        "tween_state",
        "light_source",
        "global_light",
        "texture_camera",
        "camera_layer",
        "camera_layers",
        "primary_pointer_info",
        "realm_info",
        "virtual_camera",
        "main_camera",
        "input_modifier",
        "trigger_area",
        "trigger_area_result",
        "gltf_node_modifiers",
        "skybox_time",
        "avatar_movement_info",
        "avatar_movement",
        "avatar_locomotion_settings",
        "physics_combined_force",
        "physics_combined_impulse",
        "particle_system",
    ];

    let mut sources = components
        .iter()
        .map(|component| format!("src/proto/decentraland/sdk/components/{component}.proto"))
        .collect::<Vec<_>>();

    sources.push("src/proto/decentraland/kernel/comms/rfc5/ws_comms.proto".into());
    sources.push("src/proto/decentraland/kernel/comms/rfc4/comms.proto".into());
    sources.push("src/proto/decentraland/kernel/comms/v3/archipelago.proto".into());
    sources.push("src/proto/decentraland/social/friendships/friendships.proto".into());

    let mut config = prost_build::Config::new();
    config.type_attribute(
        ".decentraland.sdk.components",
        "#[derive(serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"camelCase\")]",
    );
    // Per-type serde for types outside decentraland.sdk.components (which gets serde in bulk above).
    // These are in decentraland.common or similar packages.
    let serde_components = [
        "Vector2",
        "Vector3",
        "Color3",
        "Color4",
        "ColorRange",
        "Quaternion",
        "TextureUnion",
        "TextureUnion.tex",
        "AvatarTexture",
        "VideoTexture",
        "UiCanvasTexture",
        "Texture",
        "BorderRect",
        "FloatRange",
    ];

    for component in serde_components {
        config.type_attribute(
            component,
            "#[derive(serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"camelCase\")]",
        );
    }

    // ts-rs: inject `#[derive(ts_rs::TS)]` on exactly the proto types the system-api boundary
    // embeds (and their transitive fields), so system_api_types can export TypeScript for them.
    // Proto enum fields are stored as i32 by prost, so the enums themselves need no derive.
    let ts_components = [
        "Color3",
        "Vector2",
        "Vector3",
        "PBAvatarBase",
        "PBAvatarEquippedData",
        "PBPointerEvents.Entry",
        "PBPointerEvents.Info",
    ];
    for component in ts_components {
        config.type_attribute(component, "#[derive(ts_rs::TS)]");
    }

    let hash_components = [
        "PBMaterial",
        "GltfMaterial",
        "TextureUnion",
        "Texture",
        "AvatarTexture",
        "VideoTexture",
        "UiCanvasTexture",
    ];
    for component in hash_components {
        config.type_attribute(component, "#[derive(Hash)]");
    }

    // Emit a FileDescriptorSet alongside the generated code, so the component-schema
    // module can reflect over field/enum/oneof structure at runtime (prost-reflect).
    let descriptor_path =
        std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"))
            .join("sdk_components_descriptor.bin");
    config.file_descriptor_set_path(&descriptor_path);

    config.compile_protos(&sources, &["src/proto/"])?;

    // Generate the structural component-schema JSON from the descriptor (the curated overlay lives
    // in the editor scene now).
    let descriptor_bytes = std::fs::read(&descriptor_path).expect("read descriptor");
    let schemas_path = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"))
        .join("component_schemas.json");
    build_schema::generate(&descriptor_bytes, &schemas_path);

    for source in sources {
        println!("cargo:rerun-if-changed={source}");
    }
    println!("cargo:rerun-if-changed=build_schema.rs");

    Ok(())
}

#[cfg(feature = "social")]
fn gen_social_service() -> Result<()> {
    let mut conf = prost_build::Config::new();
    conf.service_generator(Box::new(dcl_rpc::codegen::RPCServiceGenerator::new()));
    conf.type_attribute("*", "#[derive(Debug)]");
    // Reuse common types already generated by gen_sdk_components() to avoid overwriting
    conf.extern_path(".decentraland.common", "crate::proto_components::common");
    let sources = ["src/proto/decentraland/social_service/v2/social_service_v2.proto"];
    conf.compile_protos(&sources, &["src/proto"])?;
    for source in sources {
        println!("cargo:rerun-if-changed={source}");
    }
    Ok(())
}

fn main() -> Result<()> {
    gen_sdk_components()?;
    #[cfg(feature = "social")]
    gen_social_service()?;
    Ok(())
}
