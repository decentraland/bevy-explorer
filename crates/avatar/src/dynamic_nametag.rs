use std::collections::VecDeque;

use bevy::{math::FloatOrd, prelude::*, render::primitives::Aabb};
use common::structs::AttachPoints;
use scene_runner::update_world::transform_and_parent::PostUpdateSets;

use crate::foot_ik::FootIkSet;

pub struct DynamicNametagPlugin;

impl Plugin for DynamicNametagPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            dynamic_nametag_position
                .chain()
                .after(PostUpdateSets::PlayerUpdate)
                .after(FootIkSet)
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

impl NametagHeightHistory {
    fn insert(&mut self, new_height: f32, new_timestamp: f32) {
        let index = self.heights.len();
        let old_height = self.heights.get(self.max).copied().unwrap_or(0.);

        self.heights.push_back(new_height);
        self.timestamps.push_back(new_timestamp);
        if new_height > old_height || self.max > index {
            self.max = index;
        }
    }

    fn pop_old(&mut self, threshold: f32) {
        while self
            .timestamps
            .front()
            .filter(|front| *front < &threshold)
            .is_some()
        {
            self.timestamps.pop_front();
            self.heights.pop_front();
            if self.max == 0 {
                let (max, _) = self
                    .heights
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, height)| FloatOrd(**height))
                    .unwrap_or((usize::MAX, &0.));
                self.max = max;
            } else {
                self.max -= 1;
            }
        }
    }

    fn max(&self) -> f32 {
        self.heights.get(self.max).copied().unwrap_or(0.)
    }
}

fn dynamic_nametag_position(
    attach_points_query: Query<&AttachPoints>,
    mut nametags: Query<(&mut Transform, &mut NametagHeightHistory)>,
    global_transforms: Query<&GlobalTransform>,
    aabbs: Query<&Aabb>,
    time: Res<Time<Real>>,
) {
    let new_timestamp = time.elapsed_secs_wrapped();
    let threshold = new_timestamp - 0.25;

    for attach_points in attach_points_query {
        let Ok((mut nametag_transform, mut nametag_height_history)) =
            nametags.get_mut(attach_points.nametag)
        else {
            panic!("Nametag must have Transform and NametagHeightHistory.");
        };
        let Ok(position_gt) = global_transforms.get(attach_points.position) else {
            continue;
        };
        let position = position_gt.compute_transform();

        nametag_height_history.pop_old(threshold);

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

        nametag_height_history.insert(highest_y.0, new_timestamp);

        let position_rotation = {
            let (axis, angle) = position.rotation.to_axis_angle();
            Quat::from_axis_angle(axis, -angle)
        };
        nametag_transform.translation = position_rotation
            * Vec3::new(
                head_position.translation.x - position.translation.x,
                nametag_height_history.max(),
                head_position.translation.z - position.translation.z,
            );
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
