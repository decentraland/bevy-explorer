use bevy::prelude::*;

use common::{
    sets::SceneSets,
    structs::{AttachPoints, PrimaryUser},
    util::TryInsertEx,
};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{AvatarAnchorPointType, PbAvatarAttach},
    SceneComponentId,
};
use scene_runner::update_world::{
    mesh_collider::DisableCollisions, transform_and_parent::ParentPositionSync, AddCrdtInterfaceExt,
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
) {
    for removed in removed_attachments.iter() {
        if let Some(mut commands) = commands.get_entity(removed) {
            commands.remove::<(ParentPositionSync, DisableCollisions)>();
        }
    }

    for (ent, attach) in attachments.iter() {
        let attach_points = if attach.0.avatar_id.is_none() {
            let Ok(data) = primary_user.get_single() else {
                warn!("no primary user");
                continue;
            };
            data
        } else {
            warn!("nope");
            continue;
        };

        let sync_entity = match attach.0.anchor_point_id() {
            AvatarAnchorPointType::AaptPosition => attach_points.position,
            AvatarAnchorPointType::AaptNameTag => attach_points.nametag,
            AvatarAnchorPointType::AaptLeftHand => attach_points.left_hand,
            AvatarAnchorPointType::AaptRightHand => attach_points.right_hand,
        };

        commands
            .entity(ent)
            .try_insert((ParentPositionSync(sync_entity), DisableCollisions));
        debug!("syncing {ent:?} to {sync_entity:?}");
    }
}
