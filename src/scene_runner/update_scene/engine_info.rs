use bevy::{prelude::*, core::FrameCount};

use crate::{scene_runner::{renderer_context::RendererSceneContext, SceneSets}, dcl_component::{SceneComponentId, SceneEntityId, proto_components::sdk::components::PbEngineInfo}, dcl::interface::CrdtType};

pub struct EngineInfoPlugin;

impl Plugin for EngineInfoPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(update_engine_info.in_set(SceneSets::Input));
    }
}

fn update_engine_info(
    mut scenes: Query<&mut RendererSceneContext>,
    frame: Res<FrameCount>,
) {
    for mut context in scenes.iter_mut() {
        let total_runtime = context.total_runtime;
        let tick_number = context.tick_number;

        context.update_crdt(
            SceneComponentId::ENGINE_INFO,
            CrdtType::LWW_ROOT, 
            SceneEntityId::ROOT,
            &PbEngineInfo{
                frame_number: frame.0,
                total_runtime,
                tick_number,
            }
        );
    }
}