use std::collections::VecDeque;

use bevy::{math::FloatOrd, prelude::*, render::primitives::Aabb};
use common::structs::AttachPoints;
use scene_runner::update_world::transform_and_parent::PostUpdateSets;

pub struct DynamicNametagPlugin;

impl Plugin for DynamicNametagPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            (
                drop_values_too_old,
                dynamic_nametag_position,
                insert_current_frame_value,
                smooth_out_height,
            )
                .chain()
                .after(PostUpdateSets::PlayerUpdate)
                .before(PostUpdateSets::AttachSync),
        );

        app.add_observer(add_nametag_height_history);
    }
}

#[derive(Default, Component)]
struct NametagHeightHistory {
    timestamps: VecDeque<f32>,
    heights: VecDeque<f32>,
    max: usize,
}

fn drop_values_too_old(nametags: Query<&mut NametagHeightHistory>, time: Res<Time<Real>>) {
    let threshold = time.elapsed_secs_wrapped() - 0.25;
    for mut nametag_height_history in nametags {
        while nametag_height_history
            .timestamps
            .front()
            .filter(|front| *front < &threshold)
            .is_some()
        {
            nametag_height_history.timestamps.pop_front();
            nametag_height_history.heights.pop_front();
            if nametag_height_history.max == 0 {
                let (max, _) = nametag_height_history
                    .heights
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, height)| FloatOrd(**height))
                    .unwrap_or((usize::MAX, &0.));
                nametag_height_history.max = max;
            } else {
                nametag_height_history.max -= 1;
            }
        }
    }
}

fn dynamic_nametag_position(
    attach_points_query: Query<&AttachPoints>,
    mut nametags: Query<&mut Transform>,
    global_transforms: Query<&GlobalTransform>,
    aabbs: Query<&Aabb>,
) {
    for attach_points in attach_points_query {
        let Ok(position_gt) = global_transforms.get(attach_points.position) else {
            continue;
        };
        let position = position_gt.compute_transform();

        let head_position_gt = global_transforms
            .get(attach_points.head)
            .unwrap_or(position_gt);
        let head_position = head_position_gt.compute_transform();
        let head_aabb = aabbs.get(attach_points.head).ok();

        let Some(highest_y) = [
            FloatOrd(nametag_offset(
                head_position_gt,
                &position.translation,
                head_aabb,
            )),
            // TODO extend with the heights of headgear
        ]
        .into_iter()
        .max() else {
            unreachable!("List is never empty.");
        };

        let Ok(mut nametag_transform) = nametags.get_mut(attach_points.nametag) else {
            panic!("Nametag must have Transform.");
        };
        let position_rotation = {
            let (axis, angle) = position.rotation.to_axis_angle();
            Quat::from_axis_angle(axis, -angle)
        };
        nametag_transform.translation = position_rotation
            * Vec3::new(
                head_position.translation.x - position.translation.x,
                highest_y.0,
                head_position.translation.z - position.translation.z,
            );
    }
}

fn insert_current_frame_value(
    nametags: Query<(&Transform, &mut NametagHeightHistory)>,
    time: Res<Time<Real>>,
) {
    for (transform, mut nametag_height_history) in nametags {
        let index = nametag_height_history.heights.len();
        let new_height = transform.translation.y;
        let old_height = nametag_height_history
            .heights
            .get(nametag_height_history.max)
            .copied()
            .unwrap_or(0.);

        nametag_height_history.heights.push_back(new_height);
        nametag_height_history
            .timestamps
            .push_back(time.elapsed_secs_wrapped());
        if new_height > old_height || nametag_height_history.max > index {
            nametag_height_history.max = index;
        }
    }
}

fn smooth_out_height(nametags: Query<(&mut Transform, &NametagHeightHistory)>) {
    for (mut transform, nametag_height_history) in nametags {
        if let Some(max) = nametag_height_history
            .heights
            .get(nametag_height_history.max)
            .filter(|max| **max > transform.translation.y)
        {
            transform.translation.y = *max;
        }
    }
}

fn add_nametag_height_history(
    trigger: Trigger<OnInsert, AttachPoints>,
    mut commands: Commands,
    attach_points_query: Query<&AttachPoints>,
) {
    let entity = trigger.target();

    let Ok(attach_points) = attach_points_query.get(entity) else {
        unreachable!("Infallible query.");
    };

    commands
        .entity(attach_points.nametag)
        .insert(NametagHeightHistory::default());
}

fn nametag_offset(
    global_transform: &GlobalTransform,
    root_position: &Vec3,
    maybe_aabb: Option<&Aabb>,
) -> f32 {
    let transform = global_transform.compute_transform();
    let y = transform.translation.y - root_position.y;
    if let Some(aabb) = maybe_aabb {
        let model_radius = global_transform.radius_vec3a(aabb.half_extents);
        y + model_radius + 0.125
    } else {
        y + 40. * transform.scale.y
    }
}
