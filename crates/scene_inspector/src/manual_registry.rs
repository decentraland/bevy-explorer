use bevy::prelude::*;
use dcl_component::{
    component_name_registry::{derive_component_name, make_proto_closures},
    proto_components::sdk::components::{
        PbAudioEvent, PbAvatarBase, PbAvatarEmoteCommand, PbAvatarEquippedData,
        PbAvatarMovementInfo, PbCameraMode, PbEngineInfo, PbGltfContainerLoadingState,
        PbGltfNodeState, PbPlayerIdentityData, PbPointerEventsResult, PbPointerLock,
        PbPrimaryPointerInfo, PbRaycastResult, PbRealmInfo, PbTriggerAreaResult, PbTweenState,
        PbUiCanvasInformation, PbUiDropdownResult, PbUiInputResult, PbUiScrollResult,
        PbVideoEvent,
    },
    ComponentNameRegistry, CrdtType, SceneComponentId,
};

/// Register engine→scene-only components that aren't covered by add_crdt_lww_component /
/// add_crdt_go_component auto-registration.
pub fn register_engine_components(app: &mut App) {
    let mut registry = app.world_mut().resource_mut::<ComponentNameRegistry>();

    macro_rules! reg {
        ($pb:ty, $id:expr, $crdt:expr, rw) => {{
            let (inspect, write) = make_proto_closures::<$pb>();
            registry.register(
                derive_component_name::<$pb>(),
                $id,
                $crdt,
                inspect,
                Some(write),
            );
        }};
        ($pb:ty, $id:expr, $crdt:expr, ro) => {{
            let (inspect, _write) = make_proto_closures::<$pb>();
            registry.register(derive_component_name::<$pb>(), $id, $crdt, inspect, None);
        }};
    }

    reg!(
        PbEngineInfo,
        SceneComponentId::ENGINE_INFO,
        CrdtType::LWW_ROOT,
        ro
    );
    reg!(
        PbRaycastResult,
        SceneComponentId::RAYCAST_RESULT,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbPointerEventsResult,
        SceneComponentId::POINTER_RESULT,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbPrimaryPointerInfo,
        SceneComponentId::PRIMARY_POINTER_INFO,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbPointerLock,
        SceneComponentId::POINTER_LOCK,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbCameraMode,
        SceneComponentId::CAMERA_MODE,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbTweenState,
        SceneComponentId::TWEEN_STATE,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbGltfContainerLoadingState,
        SceneComponentId::GLTF_CONTAINER_LOADING_STATE,
        CrdtType::LWW_ANY,
        ro
    );
    reg!(
        PbGltfNodeState,
        SceneComponentId::GLTF_NODE_STATE,
        CrdtType::LWW_ANY,
        ro
    );
    reg!(
        PbVideoEvent,
        SceneComponentId::VIDEO_EVENT,
        CrdtType::GO_ANY,
        ro
    );
    reg!(
        PbAudioEvent,
        SceneComponentId::AUDIO_EVENT,
        CrdtType::GO_ANY,
        ro
    );
    reg!(
        PbUiDropdownResult,
        SceneComponentId::UI_DROPDOWN_RESULT,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbUiInputResult,
        SceneComponentId::UI_INPUT_RESULT,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbUiScrollResult,
        SceneComponentId::UI_SCROLL_RESULT,
        CrdtType::LWW_ENT,
        ro
    );
    reg!(
        PbTriggerAreaResult,
        SceneComponentId::TRIGGER_AREA_RESULT,
        CrdtType::GO_ENT,
        ro
    );
    reg!(
        PbPlayerIdentityData,
        SceneComponentId::PLAYER_IDENTITY_DATA,
        CrdtType::LWW_ANY,
        ro
    );
    reg!(
        PbAvatarBase,
        SceneComponentId::AVATAR_BASE,
        CrdtType::LWW_ANY,
        ro
    );
    reg!(
        PbAvatarEquippedData,
        SceneComponentId::AVATAR_EQUIPPED_DATA,
        CrdtType::LWW_ANY,
        ro
    );
    reg!(
        PbAvatarEmoteCommand,
        SceneComponentId::AVATAR_EMOTE_COMMAND,
        CrdtType::GO_ENT,
        ro
    );
    reg!(
        PbAvatarMovementInfo,
        SceneComponentId::AVATAR_MOVEMENT_INFO,
        CrdtType::LWW_ANY,
        ro
    );

    reg!(
        PbUiCanvasInformation,
        SceneComponentId::CANVAS_INFO,
        CrdtType::LWW_ROOT,
        ro
    );
    reg!(
        PbRealmInfo,
        SceneComponentId::REALM_INFO,
        CrdtType::LWW_ROOT,
        ro
    );

    // Transform: special-cased via DclTransformAndParent (DCL binary format, not prost)
    register_transform(&mut registry);
}

fn register_transform(registry: &mut ComponentNameRegistry) {
    use dcl_component::{
        transform_and_parent::DclTransformAndParent, DclReader, DclWriter, FromDclReader,
        ToDclWriter,
    };

    let inspect = std::sync::Arc::new(|bytes: &[u8]| {
        let mut reader = DclReader::new(bytes);
        let t = DclTransformAndParent::from_reader(&mut reader)
            .map_err(|e| anyhow::anyhow!("{e:?}"))?;
        serde_json::to_string_pretty(&t).map_err(|e| anyhow::anyhow!("{e}"))
    });

    let write = std::sync::Arc::new(|json: &str| {
        let t: DclTransformAndParent =
            serde_json::from_str(json).map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut buf = Vec::new();
        let mut writer = DclWriter::new(&mut buf);
        t.to_writer(&mut writer);
        Ok(buf)
    });

    registry.register(
        "Transform".to_string(),
        SceneComponentId::TRANSFORM,
        CrdtType::LWW_ANY,
        inspect,
        Some(write),
    );
}
