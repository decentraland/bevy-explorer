use bevy::{
    app::{HierarchyPropagatePlugin, Propagate},
    prelude::*,
};

use common::{
    sets::SceneSets,
    structs::{AttachPoints, PrimaryUser},
};
use comms::profile::UserProfile;
use dcl::interface::ComponentPosition;
use dcl_component::{
    SceneComponentId, proto_components::sdk::components::{AvatarAnchorPointType, PbAvatarAttach}
};
use scene_material::{SceneMaterial, SCENE_MATERIAL_CONE_ONLY_DITHER, SCENE_MATERIAL_NO_DITHERING};
use scene_runner::update_world::{
    AddCrdtInterfaceExt, mesh_collider::DisableCollisions, transform_and_parent::{AvatarAttachStage, ParentPositionSync}, visibility::VisibilityComponent
};

pub struct AttachPlugin;

impl Plugin for AttachPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAvatarAttach, AvatarAttachment>(
            SceneComponentId::AVATAR_ATTACHMENT,
            ComponentPosition::Any,
        );
        app.add_systems(
            Update,
            (update_attached, undither_materials_on_attached_items).in_set(SceneSets::PostLoop),
        );
        app.add_plugins(HierarchyPropagatePlugin::<AttachedToPlayer>::default());
    }
}

#[derive(Component, Clone, PartialEq)]
pub struct AttachedToPlayer {
    pub is_primary: bool,
}

#[derive(Component, Debug)]
pub struct AvatarAttachment(pub PbAvatarAttach);

impl From<PbAvatarAttach> for AvatarAttachment {
    fn from(value: PbAvatarAttach) -> Self {
        Self(value)
    }
}

pub fn update_attached(
    mut commands: Commands,
    attachments: Query<(Entity, &AvatarAttachment), Changed<AvatarAttachment>>,
    mut removed_attachments: RemovedComponents<AvatarAttachment>,
    visibility_component: Query<&VisibilityComponent>,
    primary_user: Query<&AttachPoints, With<PrimaryUser>>,
    all_users: Query<(&AttachPoints, &UserProfile, Option<&PrimaryUser>)>,
) {
    for removed in removed_attachments.read() {
        if let Ok(mut commands) = commands.get_entity(removed) {
            commands.remove::<(
                ParentPositionSync<AvatarAttachStage>,
                DisableCollisions,
                Propagate<AttachedToPlayer>,
            )>();

            let required_visibility = match visibility_component.get(removed) {
                Ok(VisibilityComponent(inner)) => {
                    match inner.visible.unwrap_or(true) {
                        true => Visibility::Visible,
                        false => Visibility::Hidden,
                    }
                },
                Err(_) => Visibility::Inherited
            };

            commands.try_insert(required_visibility);
        }
    }

    for (ent, attach) in attachments.iter() {
        let (is_primary, attach_points) = match attach.0.avatar_id.as_ref() {
            None => {
                let Ok(data) = primary_user.single() else {
                    warn!("no primary user");
                    continue;
                };
                (true, data)
            }
            Some(id) => {
                let id = id.to_lowercase();
                let Some((attach, _, maybe_primary)) = all_users
                    .iter()
                    .find(|(_, profile, _)| profile.content.eth_address.to_lowercase() == id)
                else {
                    warn!("user {:?} not found", id);
                    warn!(
                        "available users: {:?}",
                        all_users
                            .iter()
                            .map(|(_, profile, _)| &profile.content)
                            .collect::<Vec<_>>()
                    );
                    continue;
                };
                (maybe_primary.is_some(), attach)
            }
        };

        let sync_entity = match attach.0.anchor_point_id() {
            AvatarAnchorPointType::AaptPosition => attach_points.position,
            AvatarAnchorPointType::AaptNameTag => attach_points.nametag,
            AvatarAnchorPointType::AaptHead => attach_points.head,
            AvatarAnchorPointType::AaptNeck => attach_points.neck,
            AvatarAnchorPointType::AaptSpine => attach_points.spine,
            AvatarAnchorPointType::AaptSpine1 => attach_points.spine_1,
            AvatarAnchorPointType::AaptSpine2 => attach_points.spine_2,
            AvatarAnchorPointType::AaptHip => attach_points.hip,
            AvatarAnchorPointType::AaptLeftShoulder => attach_points.left_shoulder,
            AvatarAnchorPointType::AaptLeftArm => attach_points.left_arm,
            AvatarAnchorPointType::AaptLeftForearm => attach_points.left_forearm,
            AvatarAnchorPointType::AaptLeftHand => attach_points.left_hand,
            AvatarAnchorPointType::AaptLeftHandIndex => attach_points.left_hand_index,
            AvatarAnchorPointType::AaptRightShoulder => attach_points.right_shoulder,
            AvatarAnchorPointType::AaptRightArm => attach_points.righ_arm,
            AvatarAnchorPointType::AaptRightForearm => attach_points.right_forearm,
            AvatarAnchorPointType::AaptRightHand => attach_points.right_hand,
            AvatarAnchorPointType::AaptRightHandIndex => attach_points.right_hand_index,
            AvatarAnchorPointType::AaptLeftUpLeg => attach_points.left_thigh,
            AvatarAnchorPointType::AaptLeftLeg => attach_points.left_shin,
            AvatarAnchorPointType::AaptLeftFoot => attach_points.left_foot,
            AvatarAnchorPointType::AaptLeftToeBase => attach_points.left_toe_base,
            AvatarAnchorPointType::AaptRightUpLeg => attach_points.right_thigh,
            AvatarAnchorPointType::AaptRightLeg => attach_points.right_shin,
            AvatarAnchorPointType::AaptRightFoot => attach_points.right_foot,
            AvatarAnchorPointType::AaptRightToeBase => attach_points.right_toe_base,
        };

        let mut commands = commands.entity(ent);
        commands.try_insert((
            ParentPositionSync::<AvatarAttachStage>::new(sync_entity),
            DisableCollisions,
            AttachedToPlayer { is_primary },
        ));
        debug!("syncing {ent:?} to {sync_entity:?}");
    }
}

#[derive(Component)]
pub struct DitheredMaterial<M: Asset>(pub Handle<M>);

#[allow(clippy::type_complexity)]
fn undither_materials_on_attached_items(
    mut commands: Commands,
    mut scene_mats: ResMut<Assets<SceneMaterial>>,
    new: Query<
        (Entity, &MeshMaterial3d<SceneMaterial>, &AttachedToPlayer),
        Without<DitheredMaterial<SceneMaterial>>,
    >,
    old: Query<(Entity, &DitheredMaterial<SceneMaterial>), Without<AttachedToPlayer>>,
) {
    for (ent, mat, attached) in new.iter() {
        let Some(mut undithered_material) = scene_mats.get(mat).cloned() else {
            continue;
        };

        if attached.is_primary {
            undithered_material.extension.data.flags |= SCENE_MATERIAL_NO_DITHERING;
        } else {
            undithered_material.extension.data.flags |= SCENE_MATERIAL_CONE_ONLY_DITHER;
        }
        let undithered_material = scene_mats.add(undithered_material);
        commands.entity(ent).try_insert((
            MeshMaterial3d(undithered_material),
            DitheredMaterial(mat.0.clone()),
        ));
    }

    for (ent, dithered) in old.iter() {
        commands
            .entity(ent)
            .try_remove::<DitheredMaterial<SceneMaterial>>()
            .try_insert(MeshMaterial3d(dithered.0.clone()));
    }
}
