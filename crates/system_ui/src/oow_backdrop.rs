//! Out of world backdrop shown while loading.
//!
//! This module provides a fullscreen backdrop that is displayed
//! when the player is out of world and removed once they are in the world.

use bevy::prelude::*;

use common::structs::ZOrder;
use scene_runner::OutOfWorld;

/// Marker component for the out of world backdrop
#[derive(Component)]
pub struct OowBackdrop;

/// Plugin that manages the out of world backdrop
pub struct OowBackdropPlugin;

impl Plugin for OowBackdropPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_oow_backdrop)
            .add_systems(Update, update_oow_backdrop.run_if(in_state(ui_core::State::Ready)));
    }
}

/// System to spawn the backdrop at startup
fn init_oow_backdrop(mut commands: Commands, asset_server: Res<AssetServer>) {
    spawn_oow_backdrop(&mut commands, &asset_server);
}

fn spawn_oow_backdrop(commands: &mut Commands, asset_server: &AssetServer) {
    commands.spawn((
        OowBackdrop,
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..Default::default()
        },
        ImageNode {
            image: asset_server.load("embedded://images/gradient-background.png"),
            ..Default::default()
        },
        ZOrder::OutOfWorldBackdrop.default(),
    ));
}

fn update_oow_backdrop(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    oow: Query<&OutOfWorld>,
    backdrop: Query<Entity, With<OowBackdrop>>,
) {
    let existing_backdrop = backdrop.single().ok();

    match (oow.is_empty(), existing_backdrop.is_some()) {
        (true, true) => {
            // in world, backdrop is showing, remove it
            commands.entity(existing_backdrop.unwrap()).despawn();
        }
        (false, false) => {
            // out of world, backdrop is not showing, spawn it
            spawn_oow_backdrop(&mut commands, &asset_server);
        }
        _ => (),
    }
}
