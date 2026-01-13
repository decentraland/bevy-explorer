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
    proto_components::sdk::components::{AvatarAnchorPointType, PbAvatarAttach},
    SceneComponentId,
};
use scene_material::{SCENE_MATERIAL_CONE_ONLY_DITHER, SCENE_MATERIAL_NO_DITHERING, SceneMaterial};
use scene_runner::update_world::{
    mesh_collider::DisableCollisions,
    transform_and_parent::{AvatarAttachStage, ParentPositionSync},
    AddCrdtInterfaceExt,
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
pub struct AttachedToPlayer{
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
            AvatarAnchorPointType::AaptLeftHand => attach_points.left_hand,
            AvatarAnchorPointType::AaptRightHand => attach_points.right_hand,
            _ => {
                warn!(
                    "unimplemented attach point {:?}",
                    attach.0.anchor_point_id()
                );
                continue;
            }
        };

        let mut commands = commands.entity(ent);
        commands.try_insert((
            ParentPositionSync::<AvatarAttachStage>::new(sync_entity),
            DisableCollisions,
            AttachedToPlayer{ is_primary },
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
