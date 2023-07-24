use bevy::prelude::*;
use common::structs::PrimaryCamera;

use crate::{renderer_context::RendererSceneContext, SceneSets};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::{common::CameraType, PbCameraMode},
    SceneComponentId, SceneEntityId,
};

pub struct CameraModePlugin;

impl Plugin for CameraModePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_camera_mode.in_set(SceneSets::Input));
    }
}

fn update_camera_mode(mut scenes: Query<&mut RendererSceneContext>, camera: Query<&PrimaryCamera>) {
    let Ok(camera) = camera.get_single() else {
        return;
    };

    let mode = if camera.distance <= 0.05 {
        CameraType::CtFirstPerson
    } else {
        CameraType::CtThirdPerson
    };

    let camera_mode = PbCameraMode { mode: mode.into() };

    for mut context in scenes.iter_mut() {
        context.update_crdt(
            SceneComponentId::CAMERA_MODE,
            CrdtType::LWW_ENT,
            SceneEntityId::CAMERA,
            &camera_mode,
        );
    }
}
