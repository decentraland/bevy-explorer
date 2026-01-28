use bevy::prelude::*;
use common::sets::SceneSets;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        pb_tween::Mode, EasingFunction, PbTween, PbTweenState, TextureMovementType,
        TweenStateStatus,
    },
    transform_and_parent::DclTransformAndParent,
    SceneComponentId,
};

use scene_material::SceneMaterial;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, ContainerEntity,
    SceneEntity,
};

#[derive(Component, Debug)]
pub struct Tween(PbTween);

impl From<PbTween> for Tween {
    fn from(value: PbTween) -> Self {
        Self(value)
    }
}

impl Tween {
    fn apply(
        &self,
        time: f32,
        transform: &mut Transform,
        maybe_h_mat: Option<&MeshMaterial3d<SceneMaterial>>,
        materials: &mut Assets<SceneMaterial>,
    ) {
        use simple_easing::*;
        use EasingFunction::*;
        let f = match self.0.easing_function() {
            EfLinear => linear,
            EfEaseinquad => quad_in,
            EfEaseoutquad => quad_out,
            EfEasequad => quad_in_out,
            EfEaseinsine => sine_in,
            EfEaseoutsine => sine_out,
            EfEasesine => sine_in_out,
            EfEaseinexpo => expo_in,
            EfEaseoutexpo => expo_out,
            EfEaseexpo => expo_in_out,
            EfEaseinelastic => elastic_in,
            EfEaseoutelastic => elastic_out,
            EfEaseelastic => elastic_in_out,
            EfEaseinbounce => bounce_in,
            EfEaseoutbounce => bounce_out,
            EfEasebounce => bounce_in_out,
            EfEaseincubic => cubic_in,
            EfEaseoutcubic => cubic_out,
            EfEasecubic => cubic_in_out,
            EfEaseinquart => quart_in,
            EfEaseoutquart => quart_out,
            EfEasequart => quart_in_out,
            EfEaseinquint => quint_in,
            EfEaseoutquint => quint_out,
            EfEasequint => quint_in_out,
            EfEaseincirc => circ_in,
            EfEaseoutcirc => circ_out,
            EfEasecirc => circ_in_out,
            EfEaseinback => back_in,
            EfEaseoutback => back_out,
            EfEaseback => back_in_out,
        };

        let ease_value = f(time);

        match &self.0.mode {
            Some(Mode::Move(data)) => {
                let start = data.start.unwrap_or_default().world_vec_to_vec3();
                let end = data.end.unwrap_or_default().world_vec_to_vec3();

                if data.face_direction == Some(true) && time == 0.0 {
                    let direction = end - start;
                    if direction == Vec3::ZERO {
                        // can't look nowhere
                    } else if direction * Vec3::new(1.0, 0.0, 1.0) != Vec3::ZERO {
                        // randomly assume +z is up for a vertical movement
                        transform.look_at(end - start, Vec3::Z);
                    } else {
                        transform.look_at(end - start, Vec3::Y);
                    }
                }

                transform.translation = start + (end - start) * ease_value;
            }
            Some(Mode::Rotate(data)) => {
                let start: Quat = data.start.unwrap_or_default().to_bevy_normalized();
                let end = data.end.unwrap_or_default().to_bevy_normalized();
                transform.rotation = start.slerp(end, ease_value);
            }
            Some(Mode::Scale(data)) => {
                let start = data.start.unwrap_or_default().abs_vec_to_vec3();
                let end = data.end.unwrap_or_default().abs_vec_to_vec3();
                transform.scale = start + ((end - start) * ease_value);
                if transform.scale.x == 0.0 {
                    transform.scale.x = f32::EPSILON;
                };
                if transform.scale.y == 0.0 {
                    transform.scale.y = f32::EPSILON;
                };
                if transform.scale.z == 0.0 {
                    transform.scale.z = f32::EPSILON;
                };
            }
            Some(Mode::TextureMove(data)) => {
                let start: Vec2 = (&data.start.unwrap_or_default()).into();
                let end: Vec2 = (&data.end.unwrap_or_default()).into();
                let Some(h_mat) = maybe_h_mat else {
                    return;
                };

                let Some(material) = materials.get_mut(h_mat) else {
                    return;
                };

                match data.movement_type() {
                    TextureMovementType::TmtOffset => {
                        material.base.uv_transform.translation =
                            (start + ((end - start) * ease_value)) * Vec2::new(1.0, -1.0);
                    }
                    TextureMovementType::TmtTiling => {
                        material.base.uv_transform.matrix2 =
                            Mat2::from_diagonal(start + ((end - start) * ease_value));
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Component, Debug, PartialEq)]
pub struct TweenState(PbTweenState);

pub struct TweenPlugin;

impl Plugin for TweenPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_crdt_lww_component::<PbTween, Tween>(
            SceneComponentId::TWEEN,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_tween.in_set(SceneSets::PostLoop));
        app.add_systems(PostUpdate, update_system_tween);
        app.add_observer(clean_scene_tween_state);
    }
}

#[allow(clippy::type_complexity)]
pub fn update_tween(
    mut commands: Commands,
    time: Res<Time>,
    mut tweens: Query<(
        Entity,
        &ContainerEntity,
        &ChildOf,
        Ref<Tween>,
        &mut Transform,
        Option<&mut TweenState>,
        Option<&MeshMaterial3d<SceneMaterial>>,
    )>,
    mut scenes: Query<&mut RendererSceneContext>,
    parents: Query<&SceneEntity>,
    materials: ResMut<Assets<SceneMaterial>>,
) {
    let materials = materials.into_inner();
    for (ent, scene_ent, parent, tween, mut transform, state, maybe_h_mat) in tweens.iter_mut() {
        let playing = tween.0.playing.unwrap_or(true);
        let delta = if playing {
            time.delta_secs() * 1000.0 / tween.0.duration
        } else {
            0.0
        };

        let updated_time = if tween.is_changed() {
            tween.0.current_time.unwrap_or(0.0)
        } else {
            state
                .as_ref()
                .map(|state| state.0.current_time + delta)
                .unwrap_or(0.0)
                .min(1.0)
        };

        let updated_status = if playing && updated_time == 1.0 {
            TweenStateStatus::TsCompleted
        } else if playing {
            TweenStateStatus::TsActive
        } else {
            TweenStateStatus::TsPaused
        };

        let updated_state = TweenState(PbTweenState {
            state: updated_status as i32,
            current_time: updated_time,
        });

        if state.as_deref() != Some(&updated_state) {
            let Ok(mut scene) = scenes.get_mut(scene_ent.root) else {
                continue;
            };

            scene.update_crdt(
                SceneComponentId::TWEEN_STATE,
                CrdtType::LWW_ENT,
                scene_ent.container_id,
                &updated_state.0,
            );

            if let Some(mut state) = state {
                state.0 = updated_state.0;
            } else {
                commands.entity(ent).try_insert(updated_state);
            }

            tween.apply(updated_time, &mut transform, maybe_h_mat, materials);

            let Ok(parent) = parents.get(parent.parent()) else {
                warn!("no parent for tweened ent");
                continue;
            };

            scene.update_crdt(
                SceneComponentId::TRANSFORM,
                CrdtType::LWW_ENT,
                scene_ent.container_id,
                &DclTransformAndParent::from_bevy_transform_and_parent(&transform, parent.id),
            );
        }
    }
}

// remove scene TWEEN_STATE data when TWEEN is removed
fn clean_scene_tween_state(
    trigger: Trigger<OnRemove, Tween>,
    mut commands: Commands,
    scene_ent: Query<&ContainerEntity>,
    mut scenes: Query<&mut RendererSceneContext>,
) {
    let entity = trigger.target();
    if let Ok(mut commands) = commands.get_entity(entity) {
        commands.try_remove::<TweenState>();
    }

    let Ok(scene_ent) = scene_ent.get(entity) else {
        return;
    };
    if let Ok(mut ctx) = scenes.get_mut(scene_ent.root) {
        ctx.clear_crdt(
            SceneComponentId::TWEEN_STATE,
            CrdtType::LWW_ANY,
            scene_ent.container_id,
        );
    }
}

#[derive(Component)]
pub struct SystemTween {
    pub target: Transform,
    pub time: f32,
}

#[derive(Component)]
pub struct SystemTweenData {
    start_pos: Transform,
    start_time: f32,
}

pub fn update_system_tween(
    mut commands: Commands,
    mut q: Query<(
        Entity,
        &mut Transform,
        Ref<SystemTween>,
        Option<&SystemTweenData>,
    )>,
    time: Res<Time>,
) {
    for (ent, mut transform, tween, data) in q.iter_mut() {
        match (tween.is_changed(), data) {
            (true, _) | (_, None) => {
                if tween.time <= 0.0 {
                    debug!("system tween instant complete @ {:?}", tween.target);
                    *transform = tween.target;
                } else {
                    debug!("system tween starting {} @ {:?}", tween.time, tween.target);
                    commands.entity(ent).try_insert(SystemTweenData {
                        start_pos: *transform,
                        start_time: time.elapsed_secs(),
                    });
                }
            }
            (false, Some(data)) => {
                let elapsed = time.elapsed_secs() - data.start_time;
                if elapsed >= tween.time {
                    debug!("system tween complete @ {:?}", tween.target);
                    *transform = tween.target;
                    commands
                        .entity(ent)
                        .remove::<SystemTween>()
                        .remove::<SystemTweenData>();
                } else {
                    let ratio = simple_easing::quad_in_out(elapsed / tween.time);
                    transform.translation = (1.0 - ratio) * data.start_pos.translation
                        + ratio * tween.target.translation;
                    transform.scale =
                        (1.0 - ratio) * data.start_pos.scale + ratio * tween.target.scale;
                    transform.rotation =
                        data.start_pos.rotation.slerp(tween.target.rotation, ratio);
                    debug!(
                        "system tween partial {}/{} @ {:?}",
                        elapsed, tween.time, transform
                    );
                }
            }
        }
    }
}
