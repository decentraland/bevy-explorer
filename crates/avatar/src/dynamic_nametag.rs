use bevy::{color::palettes, math::FloatOrd, prelude::*};
use common::structs::AttachPoints;

pub struct DynamicNametagPlugin;

impl Plugin for DynamicNametagPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            dynamic_nametag_position.after(TransformSystem::TransformPropagate),
        );
    }
}

fn dynamic_nametag_position(
    attach_points_query: Query<&AttachPoints>,
    mut transforms: Query<&mut Transform>,
    global_transforms: Query<&GlobalTransform>,
    mut gizmos: Gizmos,
) {
    for attach_points in attach_points_query {
        let Ok(position) = global_transforms
            .get(attach_points.position)
            .map(|gt| gt.compute_transform())
        else {
            continue;
        };
        gizmos.arrow(
            position.translation,
            position.translation + Vec3::Y,
            palettes::basic::RED,
        );
        let head_position = global_transforms
            .get(attach_points.head)
            .map(|gt| gt.compute_transform())
            .unwrap_or(position);
        gizmos.arrow(
            head_position.translation,
            head_position.translation + Vec3::Y,
            palettes::basic::RED,
        );
        let right_hand = global_transforms
            .get(attach_points.right_hand)
            .map(|gt| gt.compute_transform())
            .unwrap_or(position);
        gizmos.arrow(
            right_hand.translation,
            right_hand.translation + Vec3::Y,
            palettes::basic::RED,
        );
        let left_hand = global_transforms
            .get(attach_points.left_hand)
            .map(|gt| gt.compute_transform())
            .unwrap_or(position);
        gizmos.arrow(
            left_hand.translation,
            left_hand.translation + Vec3::Y,
            palettes::basic::RED,
        );

        let Some(highest_y) = [
            FloatOrd(nametag_offset(
                head_position.translation.y - position.translation.y,
                head_position.scale.y,
            )),
            FloatOrd(nametag_offset(
                right_hand.translation.y - position.translation.y,
                right_hand.scale.y,
            )),
            FloatOrd(nametag_offset(
                left_hand.translation.y - position.translation.y,
                left_hand.scale.y,
            )),
        ]
        .into_iter()
        .max() else {
            unreachable!("List is never empty.");
        };

        let Ok(mut nametag_transform) = transforms.get_mut(attach_points.nametag) else {
            panic!("Nametag must have Transform.");
        };
        nametag_transform.translation = Vec3::new(0., highest_y.0, 0.);
        gizmos.arrow(
            position.translation + nametag_transform.translation,
            position.translation + nametag_transform.translation + Vec3::Y,
            palettes::basic::OLIVE,
        );
    }
}

fn nametag_offset(y: f32, y_scale: f32) -> f32 {
    y + 40. * y_scale
}
