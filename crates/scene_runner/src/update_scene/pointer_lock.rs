use bevy::{
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use crate::{renderer_context::RendererSceneContext, SceneSets};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::PbPointerLock, SceneComponentId, SceneEntityId,
};

pub struct PointerLockPlugin;

impl Plugin for PointerLockPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_pointer_lock.in_set(SceneSets::Input));
    }
}

fn update_pointer_lock(
    mut scenes: Query<&mut RendererSceneContext>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = window.get_single() else {
        return;
    };

    let pointer_lock = PbPointerLock {
        is_pointer_locked: window.cursor.grab_mode == CursorGrabMode::Locked,
    };

    for mut context in scenes.iter_mut() {
        context.update_crdt(
            SceneComponentId::POINTER_LOCK,
            CrdtType::LWW_ENT,
            SceneEntityId::CAMERA,
            &pointer_lock,
        );
    }
}
