use bevy::{math::FloatOrd, prelude::*, render::primitives::Aabb};
use common::structs::AttachPoints;
use scene_runner::update_world::transform_and_parent::PostUpdateSets;

pub struct DynamicNametagPlugin;

impl Plugin for DynamicNametagPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            dynamic_nametag_position
                .after(PostUpdateSets::PlayerUpdate)
                .before(PostUpdateSets::AttachSync),
        );
    }
}

fn dynamic_nametag_position(
    attach_points_query: Query<&AttachPoints>,
    mut transforms: Query<&mut Transform>,
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

        let Some(highest_y) = [FloatOrd(nametag_offset(
            head_position_gt,
            &position.translation,
            head_aabb,
        ))]
        .into_iter()
        .max() else {
            unreachable!("List is never empty.");
        };

        let Ok(mut nametag_transform) = transforms.get_mut(attach_points.nametag) else {
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
