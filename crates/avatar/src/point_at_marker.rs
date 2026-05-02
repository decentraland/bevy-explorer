use bevy::{platform::collections::HashMap, prelude::*};
use common::{
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
        app.add_systems(
            Update,
            (
                sync_markers,
                update_marker_images,
                position_markers.after(sync_markers),
            ),
        );
    }
}

/// Marker visibility cutoff (metres from camera) — beyond this we hide rather
/// than letting the marker shrink into a dot.
const MAX_VISIBLE_DISTANCE: f32 = 100.0;

/// Reference distance (metres) at which the marker is drawn at base size.
/// Closer than this and we clamp; further and the marker shrinks linearly.
const REFERENCE_DISTANCE: f32 = 5.0;

/// Marker base diameter in pixels at `REFERENCE_DISTANCE`.
const BASE_DIAMETER_PX: f32 = 64.0;

/// Marker minimum and maximum diameters (pixels).
const MIN_DIAMETER_PX: f32 = 16.0;
const MAX_DIAMETER_PX: f32 = 96.0;

#[derive(Resource, Default)]
struct MarkerOverlay {
    root: Option<Entity>,
}

#[derive(Component)]
struct PointAtMarkerOverlay;

#[derive(Component)]
struct PointAtMarker {
    avatar: Entity,
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
        (With<AvatarShape>, Or<(With<PrimaryUser>, With<ForeignPlayer>)>),
    >,
    markers: Query<(Entity, &PointAtMarker)>,
) {
    let Some(root) = overlay.root else {
        return;
    };

    let mut existing: HashMap<Entity, Entity> =
        markers.iter().map(|(m, p)| (p.avatar, m)).collect();

    for (avatar_ent, sync, foreign, profile) in &avatars {
        if !sync.is_pointing {
            continue;
        }
        let Some(address) = foreign
            .map(|f| f.address)
            .or_else(|| profile.and_then(|p| p.content.eth_address.as_h160()))
        else {
            continue;
        };

        if existing.remove(&avatar_ent).is_some() {
            continue;
        }

        let bg = name_color(address);
        commands.entity(root).with_children(|parent| {
            parent
                .spawn((
                    PointAtMarker { avatar: avatar_ent },
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(BASE_DIAMETER_PX),
                        height: Val::Px(BASE_DIAMETER_PX),
                        display: Display::None,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..Default::default()
                    },
                    BackgroundColor(bg),
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

    for marker_ent in existing.values().copied() {
        commands.entity(marker_ent).despawn();
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
    primary_camera: Single<(&Camera, &GlobalTransform), With<PrimaryCamera>>,
    avatars: Query<&PointAtSync>,
    mut markers: Query<(Entity, &PointAtMarker, &mut Node)>,
) {
    let (camera, camera_transform) = primary_camera.into_inner();

    for (marker_ent, marker, mut node) in markers.iter_mut() {
        let Ok(sync) = avatars.get(marker.avatar) else {
            commands.entity(marker_ent).despawn();
            continue;
        };
        if !sync.is_pointing {
            continue;
        }
        let target_bevy = DclTranslation([
            sync.target_world.x,
            sync.target_world.y,
            sync.target_world.z,
        ])
        .to_bevy_translation();

        let distance = (target_bevy - camera_transform.translation()).length();
        if distance > MAX_VISIBLE_DISTANCE {
            node.display = Display::None;
            continue;
        }

        let Ok(viewport) = camera.world_to_viewport_with_depth(camera_transform, target_bevy)
        else {
            node.display = Display::None;
            continue;
        };
        if viewport.z <= 0.0 {
            node.display = Display::None;
            continue;
        }

        let scale = REFERENCE_DISTANCE / distance.max(REFERENCE_DISTANCE * 0.5);
        let diameter = (BASE_DIAMETER_PX * scale).clamp(MIN_DIAMETER_PX, MAX_DIAMETER_PX);

        node.display = Display::Flex;
        node.width = Val::Px(diameter);
        node.height = Val::Px(diameter);
        node.left = Val::Px(viewport.x - diameter * 0.5);
        node.top = Val::Px(viewport.y - diameter * 0.5);
    }
}
