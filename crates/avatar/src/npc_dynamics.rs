use bevy::{platform::collections::HashMap, prelude::*};

use common::structs::{AvatarDynamicState, PrimaryUser};

use comms::global_crdt::ForeignPlayer;
use scene_runner::{renderer_context::RendererSceneContext, ContainerEntity};

use crate::AvatarShape;

pub struct NpcMovementPlugin;

impl Plugin for NpcMovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_npc_velocity);
    }
}

#[allow(clippy::type_complexity)]
fn update_npc_velocity(
    mut commands: Commands,
    npcs: Query<
        (Entity, &ContainerEntity, &GlobalTransform),
        (
            With<AvatarShape>,
            Without<ForeignPlayer>,
            Without<PrimaryUser>,
        ),
    >,
    scenes: Query<&RendererSceneContext>,
    mut saved_positions: Local<HashMap<Entity, (u32, Vec3)>>,
) {
    let mut last_positions = std::mem::take(&mut *saved_positions);

    for (ent, container, gt) in npcs.iter() {
        let current_translation = gt.translation();
        let (last_tick, prev_translation) = last_positions
            .remove(&ent)
            .unwrap_or((0, current_translation));
        let Ok(scene) = scenes.get(container.root) else {
            continue;
        };

        saved_positions.insert(ent, (scene.tick_number, current_translation));

        if scene.tick_number == last_tick {
            continue;
        }

        let velocity = (current_translation - prev_translation) / scene.last_update_dt;

        commands.entity(ent).insert(AvatarDynamicState {
            velocity: if velocity.is_finite() {
                velocity
            } else {
                Vec3::ZERO
            },
            ground_height: 0.0,
            ..Default::default()
        });
    }
}
