use bevy::prelude::*;
use common::structs::{CameraOverride, PrimaryCamera};

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
    let Ok(camera) = camera.single() else {
        return;
    };

    let distance = match camera.scene_override {
        Some(CameraOverride::Distance(d)) => d,
        _ => camera.distance,
    };

    let mode = match camera.scene_override {
        Some(CameraOverride::Cinematic(_)) => CameraType::CtCinematic,
        _ => {
            if distance <= 0.05 {
                CameraType::CtFirstPerson
            } else {
                CameraType::CtThirdPerson
            }
        }
    };

    let camera_mode = PbCameraMode { mode: mode.into() };

    for mut context in scenes.iter_mut() {
        context.update_crdt_if_different(
            SceneComponentId::CAMERA_MODE,
            CrdtType::LWW_ENT,
            SceneEntityId::CAMERA,
            &camera_mode,
        );
    }
}
