use bevy::prelude::*;

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
        app.add_systems(Update, update_attached.in_set(SceneSets::PostLoop));
    }
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
    all_users: Query<(&AttachPoints, &UserProfile)>,
) {
    for removed in removed_attachments.read() {
        if let Some(mut commands) = commands.get_entity(removed) {
            commands.remove::<(ParentPositionSync<AvatarAttachStage>, DisableCollisions)>();
        }
    }

    for (ent, attach) in attachments.iter() {
        let attach_points = match attach.0.avatar_id.as_ref() {
            None => {
                let Ok(data) = primary_user.single() else {
                    warn!("no primary user");
                    continue;
                };
                data
            }
            Some(id) => {
                let Some((attach, _)) = all_users
                    .iter()
                    .find(|(_, profile)| profile.content.user_id.as_ref() == Some(id))
                else {
                    warn!("user {:?} not found", id);
                    warn!(
                        "available users: {:?}",
                        all_users
                            .iter()
                            .map(|(_, profile)| &profile.content)
                            .collect::<Vec<_>>()
                    );
                    continue;
                };
                attach
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

        commands.entity(ent).try_insert((
            ParentPositionSync::<AvatarAttachStage>::new(sync_entity),
            DisableCollisions,
        ));
        debug!("syncing {ent:?} to {sync_entity:?}");
    }
}
