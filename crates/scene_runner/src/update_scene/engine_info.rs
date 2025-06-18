use bevy::{diagnostic::FrameCount, prelude::*};

use crate::{renderer_context::RendererSceneContext, SceneSets};
use dcl::interface::CrdtType;
use dcl_component::{
    proto_components::sdk::components::PbEngineInfo, SceneComponentId, SceneEntityId,
};

pub struct EngineInfoPlugin;

impl Plugin for EngineInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_engine_info.in_set(SceneSets::Input));
    }
}

fn update_engine_info(mut scenes: Query<&mut RendererSceneContext>, frame: Res<FrameCount>) {
    for mut context in scenes.iter_mut() {
        let total_runtime = context.total_runtime;
        let tick_number = context.tick_number;

        context.update_crdt(
            SceneComponentId::ENGINE_INFO,
            CrdtType::LWW_ROOT,
            SceneEntityId::ROOT,
            &PbEngineInfo {
                frame_number: frame.0,
                total_runtime,
                tick_number,
            },
        );
    }
}
