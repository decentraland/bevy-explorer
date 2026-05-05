use bevy::{platform::collections::HashMap, prelude::*, ui::UiSystem};
use common::{
    sets::PostUpdateSets,
    structs::{PointAtSync, PrimaryCamera, PrimaryUser, ZOrder},
    util::AsH160,
};
use comms::{
    global_crdt::ForeignPlayer,
    profile::{ProfileManager, UserProfile},
};
use dcl_component::transform_and_parent::DclTranslation;
use ethers_core::types::Address;

use crate::{name_color::name_color, AvatarShape};

pub struct PointAtMarkerPlugin;

impl Plugin for PointAtMarkerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MarkerOverlay>();
        app.add_systems(Startup, setup_overlay);
        // Run alongside the IK chain so we read the live (post-CameraUpdate)
        // camera transform — running in `Update` would see last frame's
        // camera pose. Also `.before(UiSystem::Layout)` because UI layout
        // runs in `PostUpdate` before the default `TransformPropagate` and is
        // otherwise unordered relative to our `InverseKinematics` set —
        // without the explicit edge, layout could pick up our node writes a
        // frame late.
        app.add_systems(
            PostUpdate,
            (sync_markers, update_marker_images, position_markers)
                .chain()
                .in_set(PostUpdateSets::InverseKinematics)
                .before(UiSystem::Layout),
        );
    }
}

/// Marker visibility cutoff (metres from camera) — beyond this we hide rather
/// than letting the marker shrink into a dot.
const MAX_VISIBLE_DISTANCE: f32 = 100.0;

/// Reference distance (metres) at which the marker is drawn at base size.
/// Closer than this and we clamp; further and the marker shrinks linearly.
const REFERENCE_DISTANCE: f32 = 5.0;

/// Marker diameters expressed as a percentage of the viewport's smaller
/// dimension, so the on-screen size is consistent across aspect ratios.
const BASE_DIAMETER_PCT: f32 = 6.0;
const MIN_DIAMETER_PCT: f32 = 1.5;
const MAX_DIAMETER_PCT: f32 = 9.0;

/// Exponential-smoothing time constant (seconds) for the marker's fade.
/// Matches Unity's ~0.3s fade duration when applied as a τ for an
/// exponential ramp toward 1.0/0.0.
const FADE_TAU: f32 = 0.12;

/// Below this fade level a fading-out marker is despawned entirely.
const FADE_DESPAWN_THRESHOLD: f32 = 0.01;

#[derive(Resource, Default)]
struct MarkerOverlay {
    root: Option<Entity>,
}

#[derive(Component)]
struct PointAtMarkerOverlay;

#[derive(Component)]
struct PointAtMarker {
    avatar: Entity,
    bg: Color,
    /// Smoothed [0, 1] visibility. Ramps toward 1 while the source avatar is
    /// pointing and toward 0 once it stops; the marker is despawned once it
    /// reaches `FADE_DESPAWN_THRESHOLD` so the ramp-out plays through.
    fade: f32,
}

#[derive(Component)]
struct MarkerImageNode;

#[derive(Component)]
struct PendingMarkerImage(Address);

fn setup_overlay(mut commands: Commands, mut overlay: ResMut<MarkerOverlay>) {
    let root = commands
        .spawn((
            PointAtMarkerOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            Pickable::IGNORE,
            ZOrder::PointAtMarker.default(),
        ))
        .id();
    overlay.root = Some(root);
}

#[allow(clippy::type_complexity)]
fn sync_markers(
    mut commands: Commands,
    overlay: Res<MarkerOverlay>,
    avatars: Query<
        (
            Entity,
            &PointAtSync,
            Option<&ForeignPlayer>,
            Option<&UserProfile>,
        ),
        (
            With<AvatarShape>,
            Or<(With<PrimaryUser>, With<ForeignPlayer>)>,
        ),
    >,
    markers: Query<(Entity, &PointAtMarker)>,
) {
    let Some(root) = overlay.root else {
        return;
    };

    let existing: HashMap<Entity, Entity> = markers.iter().map(|(m, p)| (p.avatar, m)).collect();

    for (avatar_ent, sync, foreign, profile) in &avatars {
        if !sync.is_pointing || existing.contains_key(&avatar_ent) {
            continue;
        }
        let Some(address) = foreign
            .map(|f| f.address)
            .or_else(|| profile.and_then(|p| p.content.eth_address.as_h160()))
        else {
            continue;
        };

        let bg = name_color(address);
        commands.entity(root).with_children(|parent| {
            parent
                .spawn((
                    PointAtMarker {
                        avatar: avatar_ent,
                        bg,
                        fade: 0.0,
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(BASE_DIAMETER_PCT),
                        height: Val::Percent(BASE_DIAMETER_PCT),
                        display: Display::None,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..Default::default()
                    },
                    BackgroundColor(bg.with_alpha(0.0)),
                    BorderRadius::MAX,
                    PendingMarkerImage(address),
                    Pickable::IGNORE,
                ))
                .with_children(|inner| {
                    inner.spawn((
                        MarkerImageNode,
                        Node {
                            width: Val::Percent(85.0),
                            height: Val::Percent(85.0),
                            ..Default::default()
                        },
                        BorderRadius::MAX,
                        Pickable::IGNORE,
                    ));
                });
        });
    }
}

fn update_marker_images(
    mut commands: Commands,
    mut profiles: ProfileManager,
    pending: Query<(Entity, &PendingMarkerImage, &Children)>,
    mut images: Query<&mut ImageNode, With<MarkerImageNode>>,
    image_owners: Query<Entity, With<MarkerImageNode>>,
) {
    for (marker_ent, pending_image, children) in &pending {
        match profiles.get_image(pending_image.0) {
            Err(_) => {
                commands.entity(marker_ent).remove::<PendingMarkerImage>();
            }
            Ok(Some(handle)) => {
                let image_child = children.iter().find(|c| image_owners.get(*c).is_ok());
                if let Some(image_ent) = image_child {
                    if let Ok(mut image_node) = images.get_mut(image_ent) {
                        image_node.image = handle;
                    } else {
                        commands.entity(image_ent).insert(ImageNode::new(handle));
                    }
                    commands.entity(marker_ent).remove::<PendingMarkerImage>();
                }
            }
            Ok(None) => (),
        }
    }
}

fn position_markers(
    mut commands: Commands,
    time: Res<Time>,
    primary_camera: Single<(Entity, &Camera), With<PrimaryCamera>>,
    gt_helper: bevy::transform::helper::TransformHelper,
    avatars: Query<&PointAtSync>,
    mut markers: Query<(
        Entity,
        &mut PointAtMarker,
        &mut Node,
        &mut BackgroundColor,
        &Children,
    )>,
    mut images: Query<&mut ImageNode, With<MarkerImageNode>>,
) {
    let (camera_entity, camera) = primary_camera.into_inner();
    // The camera moves in `Update` but its `GlobalTransform` is only refreshed
    // by `TransformPropagate` in `PostUpdate`, so reading the cached
    // `GlobalTransform` here gives last frame's pose — visible as a one-frame
    // marker lag during fast camera moves. Recompute on the fly from the live
    // `Transform` chain instead.
    let Ok(camera_transform) = gt_helper.compute_global_transform(camera_entity) else {
        return;
    };
    let Some(viewport_size) = camera.logical_viewport_size() else {
        return;
    };

    let dt = time.delta_secs();
    let alpha_step = if dt > 0.0 {
        1.0 - (-dt / FADE_TAU).exp()
    } else {
        0.0
    };

    for (marker_ent, mut marker, mut node, mut bg, children) in markers.iter_mut() {
        // Resolve target fade from the source avatar's pointing state. A
        // missing avatar (despawned/wearable swap) ramps out the same way as
        // a normal release.
        let target_fade = match avatars.get(marker.avatar) {
            Ok(sync) if sync.is_pointing => 1.0,
            _ => 0.0,
        };
        marker.fade += (target_fade - marker.fade) * alpha_step;

        // Despawn once we've ramped down past the despawn threshold. Mid-fade
        // we keep the marker alive and visible, modulating only its alpha.
        if target_fade <= 0.0 && marker.fade < FADE_DESPAWN_THRESHOLD {
            commands.entity(marker_ent).despawn();
            continue;
        }

        let alpha = marker.fade.clamp(0.0, 1.0);
        bg.0 = marker.bg.with_alpha(alpha);
        for child in children.iter() {
            if let Ok(mut image) = images.get_mut(child) {
                image.color = Color::WHITE.with_alpha(alpha);
            }
        }

        // Pull the latest target from the avatar (if still pointing) for
        // positioning; otherwise hold position while we ramp out.
        let Some(target_bevy) = avatars
            .get(marker.avatar)
            .ok()
            .filter(|s| s.is_pointing)
            .map(|sync| {
                DclTranslation([
                    sync.target_world.x,
                    sync.target_world.y,
                    sync.target_world.z,
                ])
                .to_bevy_translation()
            })
        else {
            continue;
        };

        let distance = (target_bevy - camera_transform.translation()).length();
        if distance > MAX_VISIBLE_DISTANCE {
            node.display = Display::None;
            continue;
        }

        let Ok(projected) = camera.world_to_viewport_with_depth(&camera_transform, target_bevy)
        else {
            node.display = Display::None;
            continue;
        };
        if projected.z <= 0.0 {
            node.display = Display::None;
            continue;
        }

        // Diameter is a percentage of the viewport's smaller dimension —
        // `Val::Percent` on width/height resolves against the matching parent
        // axis, so we keep things isotropic by referencing the same axis for
        // both, and we offset left/top in the matching dimension's percent.
        let scale = REFERENCE_DISTANCE / distance.max(REFERENCE_DISTANCE * 0.5);
        let diameter_pct = (BASE_DIAMETER_PCT * scale).clamp(MIN_DIAMETER_PCT, MAX_DIAMETER_PCT);
        let short_side = viewport_size.x.min(viewport_size.y);
        let diameter_px = diameter_pct * 0.01 * short_side;
        let half_w_pct = (diameter_px * 0.5 / viewport_size.x) * 100.0;
        let half_h_pct = (diameter_px * 0.5 / viewport_size.y) * 100.0;
        let left_pct = (projected.x / viewport_size.x) * 100.0 - half_w_pct;
        let top_pct = (projected.y / viewport_size.y) * 100.0 - half_h_pct;
        let width_pct = diameter_pct * short_side / viewport_size.x;
        let height_pct = diameter_pct * short_side / viewport_size.y;

        node.display = Display::Flex;
        node.width = Val::Percent(width_pct);
        node.height = Val::Percent(height_pct);
        node.left = Val::Percent(left_pct);
        node.top = Val::Percent(top_pct);
    }
}
