//! Loading screen shown while SystemScene is loading.
//!
//! This module provides a fullscreen loading background that is displayed
//! when a SystemScene is defined and removed once it's fully initialized
//! (i.e., when the scene has rendered UiBackground images).

use bevy::{asset::LoadState, prelude::*};

use common::structs::SystemScene;
use ui_core::nine_slice::Ui9Slice;

use crate::initialize_scene::{SceneLoading, SuperUserScene};
use crate::update_world::scene_ui::ui_background::UiBackgroundMarker;
use crate::SceneEntity;

/// Marker component for the loading screen shown while SystemScene loads
#[derive(Component)]
pub struct SystemSceneLoadingScreen;

/// Plugin that manages the SystemScene loading screen
pub struct LoadingSystemSceneScreenPlugin;

impl Plugin for LoadingSystemSceneScreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_system_scene_loading_screen)
            .add_systems(Update, remove_system_scene_loading_screen);
    }
}

/// Spawns a fullscreen loading background when SystemScene is defined
fn spawn_system_scene_loading_screen(
    mut commands: Commands,
    system_scene: Option<Res<SystemScene>>,
    asset_server: Res<AssetServer>,
) {
    // Only spawn if SystemScene is defined with a source
    if system_scene.map(|s| s.source.is_some()).unwrap_or(false) {
        commands.spawn((
            SystemSceneLoadingScreen,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..Default::default()
            },
            // Use the embedded background image
            ImageNode {
                image: asset_server.load("embedded://images/loading_background.png"),
                ..Default::default()
            },
            GlobalZIndex(i32::MAX), // Ensure it's on top of everything
        ));
        info!("Spawned SystemScene loading screen");
    }
}

/// State for tracking when to remove the loading screen
#[derive(Default)]
struct LoadingScreenState {
    /// Number of frames since we detected loaded UI backgrounds
    frames_since_ready: u32,
}

/// Removes the loading screen once the SystemScene has rendered UiBackground images
fn remove_system_scene_loading_screen(
    mut commands: Commands,
    loading_screen: Query<Entity, With<SystemSceneLoadingScreen>>,
    system_scene: Option<Res<SystemScene>>,
    asset_server: Res<AssetServer>,
    // SuperUserScene entities (scene roots) that are initialized
    super_user_scenes: Query<Entity, (With<SuperUserScene>, Without<SceneLoading>)>,
    // UiBackgroundMarker entities (the actual rendered backgrounds)
    ui_background_markers: Query<Entity, With<UiBackgroundMarker>>,
    // To get the scene entity from UI hierarchy
    scene_entities: Query<&SceneEntity>,
    // Images (Ui9Slice or ImageNode)
    nine_slices: Query<&Ui9Slice>,
    image_nodes: Query<&ImageNode>,
    // Local state to track frames
    mut state: Local<LoadingScreenState>,
) {
    // Early exit if no loading screen exists
    if loading_screen.is_empty() {
        return;
    }

    // If no SystemScene is defined, remove loading screen
    let has_system_scene = system_scene.map(|s| s.source.is_some()).unwrap_or(false);
    if !has_system_scene {
        for entity in loading_screen.iter() {
            commands.entity(entity).despawn();
        }
        return;
    }

    // Check if any SuperUserScene has loaded UiBackground images
    for super_user_entity in super_user_scenes.iter() {
        // Find UiBackgroundMarker entities that belong to this SuperUserScene
        for bg_entity in ui_background_markers.iter() {
            // Try to find the SceneEntity by traversing up or checking the entity
            let belongs_to_super_user = scene_entities
                .iter()
                .any(|se| se.root == super_user_entity);

            if !belongs_to_super_user {
                continue;
            }

            // Check if the background image is loaded
            let image_loaded = if let Ok(nine_slice) = nine_slices.get(bg_entity) {
                matches!(
                    asset_server.load_state(&nine_slice.image),
                    LoadState::Loaded
                )
            } else if let Ok(image_node) = image_nodes.get(bg_entity) {
                matches!(
                    asset_server.load_state(&image_node.image),
                    LoadState::Loaded
                )
            } else {
                // No image, just a color background - consider it ready
                true
            };

            if image_loaded {
                state.frames_since_ready += 1;

                // Wait 1 frame after detecting loaded images
                if state.frames_since_ready >= 2 {
                    for entity in loading_screen.iter() {
                        info!("SystemScene UI backgrounds loaded, removing loading screen");
                        commands.entity(entity).despawn();
                    }
                    state.frames_since_ready = 0;
                }
                return;
            }
        }
    }

    // Reset counter if conditions not met
    state.frames_since_ready = 0;
}