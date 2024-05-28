use bevy::prelude::*;

use common::{
    dynamics::{PLAYER_COLLIDER_HEIGHT, PLAYER_COLLIDER_RADIUS},
    sets::SceneSets,
    structs::{AvatarControl, PrimaryUser},
};
use comms::global_crdt::ForeignPlayer;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{
        AvatarControlType, AvatarModifierType, PbAvatarModifierArea,
    },
    SceneComponentId,
};
use wallet::Wallet;

use crate::{ContainingScene, SceneEntity};

use super::AddCrdtInterfaceExt;

pub struct AvatarModifierAreaPlugin;

#[derive(Component, Debug)]
pub struct AvatarModifierArea(pub PbAvatarModifierArea);

impl From<PbAvatarModifierArea> for AvatarModifierArea {
    fn from(value: PbAvatarModifierArea) -> Self {
        Self(value)
    }
}

impl Plugin for AvatarModifierAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbAvatarModifierArea, AvatarModifierArea>(
            SceneComponentId::AVATAR_MODIFIER_AREA,
            ComponentPosition::Any,
        );

        app.add_systems(
            Update,
            (
                update_avatar_modifier_area,
                manage_foreign_player_visibility,
            )
                .chain()
                .in_set(SceneSets::PostLoop),
        );
    }
}

#[derive(Component, Default)]
pub struct PlayerModifiers {
    pub hide: bool,
    pub hide_profile: bool,
    pub walk_speed: Option<f32>,
    pub run_speed: Option<f32>,
    pub friction: Option<f32>,
    pub gravity: Option<f32>,
    pub jump_height: Option<f32>,
    pub fall_speed: Option<f32>,
    pub control_type: Option<AvatarControl>,
    pub turn_speed: Option<f32>,
    pub block_weighted_movement: bool,
    pub areas: Vec<Entity>,
}

impl PlayerModifiers {
    pub fn combine(&self, user: &PrimaryUser) -> PrimaryUser {
        PrimaryUser {
            walk_speed: self.walk_speed.unwrap_or(user.walk_speed),
            run_speed: self.run_speed.unwrap_or(user.run_speed),
            friction: self.friction.unwrap_or(user.friction),
            gravity: self.gravity.unwrap_or(user.gravity),
            jump_height: self.jump_height.unwrap_or(user.jump_height),
            fall_speed: self.fall_speed.unwrap_or(user.fall_speed),
            control_type: self.control_type.unwrap_or(user.control_type),
            turn_speed: self.turn_speed.unwrap_or(user.turn_speed),
            block_weighted_movement: self.block_weighted_movement || user.block_weighted_movement,
        }
    }
}

#[allow(clippy::type_complexity)]
fn update_avatar_modifier_area(
    mut commands: Commands,
    mut players: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&ForeignPlayer>,
            Option<&mut PlayerModifiers>,
        ),
        Or<(With<PrimaryUser>, With<ForeignPlayer>)>,
    >,
    containing_scene: ContainingScene,
    areas: Query<(Entity, &SceneEntity, &AvatarModifierArea, &GlobalTransform)>,
    me: Res<Wallet>,
) {
    // for every player
    for (player, gt, maybe_foreign, maybe_modifiers) in players.iter_mut() {
        let Some(mut modifiers) = maybe_modifiers else {
            commands.entity(player).insert(PlayerModifiers::default());
            continue;
        };

        // reset overrides
        *modifiers = PlayerModifiers {
            areas: std::mem::take(&mut modifiers.areas),
            ..PlayerModifiers::default()
        };

        let player_position = gt.translation();
        let player_id = format!(
            "{:#x}",
            maybe_foreign
                .as_ref()
                .map(|f| f.address)
                .unwrap_or(me.address().unwrap_or_default())
        );

        // utility to check if player is within a camera area
        let player_in_area = |area: &AvatarModifierArea, transform: &GlobalTransform| -> bool {
            // check exclusions
            if area.0.exclude_ids.contains(&player_id) {
                return false;
            }

            // check bounds
            let (_, rotation, translation) = transform.to_scale_rotation_translation();
            let player_relative_position = rotation.inverse() * (player_position - translation);
            let area = area.0.area.unwrap_or_default().abs_vec_to_vec3() * 0.5
                + Vec3::new(
                    PLAYER_COLLIDER_RADIUS,
                    PLAYER_COLLIDER_HEIGHT,
                    PLAYER_COLLIDER_RADIUS,
                ) * if area.0.use_collider_range.unwrap_or(true) {
                    1.0
                } else {
                    0.0
                };
            player_relative_position.clamp(-area, area) == player_relative_position
        };

        let scenes = containing_scene.get_area(player, PLAYER_COLLIDER_RADIUS);

        // gather areas
        for (ent, scene_ent, area, transform) in areas.iter() {
            let current_index = modifiers
                .areas
                .iter()
                .enumerate()
                .find(|(_, e)| ent == **e)
                .map(|(ix, _)| ix);
            let in_area = scenes.contains(&scene_ent.root) && player_in_area(area, transform);

            if in_area == current_index.is_some() {
                continue;
            }

            match current_index {
                // remove if no longer in area
                Some(index) => {
                    modifiers.areas.remove(index);
                }
                // add at end if newly entered
                None => modifiers.areas.push(ent),
            }
        }

        // remove destroyed areas
        modifiers
            .areas
            .retain(|area_ent| areas.get(*area_ent).is_ok());

        // for each modifier area the player is within (starting from oldest)
        for area in modifiers.areas.clone().iter() {
            let (_, _, area, _) = areas.get(*area).unwrap();

            // apply modifiers
            modifiers.hide |= area
                .0
                .modifiers
                .contains(&(AvatarModifierType::AmtHideAvatars as i32));
            modifiers.hide_profile |= area
                .0
                .modifiers
                .contains(&(AvatarModifierType::AmtDisablePassports as i32));

            if let Some(ref movement) = area.0.movement_settings {
                if movement.control_mode.is_some() {
                    modifiers.control_type = Some(match movement.control_mode() {
                        AvatarControlType::CctNone => AvatarControl::None,
                        AvatarControlType::CctRelative => AvatarControl::Relative,
                        AvatarControlType::CctTank => AvatarControl::Tank,
                    })
                }

                modifiers.run_speed = movement.run_speed.or(modifiers.run_speed);
                modifiers.friction = movement.friction.or(modifiers.friction);
                modifiers.gravity = movement.gravity.or(modifiers.gravity);
                modifiers.jump_height = movement.jump_height.or(modifiers.jump_height);
                modifiers.fall_speed = movement.max_fall_speed.or(modifiers.fall_speed);
                modifiers.turn_speed = movement.turn_speed.or(modifiers.turn_speed);
                modifiers.walk_speed = movement.walk_speed.or(modifiers.walk_speed);
                modifiers.block_weighted_movement |=
                    !(movement.allow_weighted_movement.unwrap_or(true));
            }
        }
    }
}

// primary user visiblity is more complex, and is managed in user_input::manage_player_visibility
fn manage_foreign_player_visibility(
    mut players: Query<(&mut Visibility, &PlayerModifiers), With<ForeignPlayer>>,
) {
    for (mut vis, modifiers) in players.iter_mut() {
        #[allow(clippy::collapsible_else_if)]
        if modifiers.hide {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        } else {
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }
    }
}
