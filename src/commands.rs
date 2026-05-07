use bevy::prelude::*;
use bevy_console::ConsoleCommand;
use common::structs::{AppConfig, PreviewMode, PrimaryUser, SceneLoadDistance};
use scene_runner::{
    initialize_scene::{parcels_in_range, ScenePointers},
    OutOfWorld,
};

// TODO move these somewhere better
/// set location
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/teleport")]
pub struct ChangeLocationCommand {
    #[arg(allow_hyphen_values(true))]
    x: i32,
    #[arg(allow_hyphen_values(true))]
    y: i32,
}

pub fn change_location(
    mut commands: Commands,
    mut input: ConsoleCommand<ChangeLocationCommand>,
    mut player: Query<(Entity, &mut Transform), With<PrimaryUser>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok((ent, mut transform)) = player.single_mut() {
            transform.translation.x = command.x as f32 * 16.0 + 8.0;
            transform.translation.z = -command.y as f32 * 16.0 - 8.0;
            if let Ok(mut commands) = commands.get_entity(ent) {
                commands.try_insert(OutOfWorld);
            }
            input.reply_ok(format!("new location: {:?}", (command.x, command.y)));
            return;
        }

        input.reply_failed("failed to set location");
    }
}

/// set scene load distance (defaults to 75.0m) and additional unload distance (defaults to 25.0m)
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_distance")]
pub struct SceneDistanceCommand {
    distance: Option<f32>,
    unload: Option<f32>,
}

pub fn scene_distance(
    mut input: ConsoleCommand<SceneDistanceCommand>,
    mut scene_load_distance: ResMut<SceneLoadDistance>,
) {
    if let Some(Ok(command)) = input.take() {
        let distance = command.distance.unwrap_or(75.0);
        scene_load_distance.load = distance;
        if let Some(unload) = command.unload {
            scene_load_distance.unload = unload;
        }
        input.reply_ok(format!(
            "set scene load distance to +{distance} -{}",
            scene_load_distance.load + scene_load_distance.unload
        ));
    }
}

/// Locks the preview mode to the current parcel
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/lock_preview")]
pub struct LockPreviewCommand;

pub fn lock_preview(
    mut input: ConsoleCommand<LockPreviewCommand>,
    mut preview_mode: ResMut<PreviewMode>,
    focus: Single<&GlobalTransform, With<PrimaryUser>>,
    pointers: Res<ScenePointers>,
) {
    if let Some(Ok(_command)) = input.take() {
        let Some((parcel, _)) = parcels_in_range(&focus, 0.0, pointers.min(), pointers.max()).pop()
        else {
            unreachable!("Player should never be in a invalid parcel.");
        };
        let Some(_current_scene) = pointers.get(parcel) else {
            input.reply_failed(format!("failed to locked preview to parcel {}", parcel));
            return;
        };
        preview_mode.preview_parcel = Some(parcel);

        input.reply_ok(format!("locked preview to parcel {}", parcel));
    }
}

/// Unlocks the preview mode to the current parcel
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/unlock_preview")]
pub struct UnlockPreviewCommand;

pub fn unlock_preview(
    mut input: ConsoleCommand<UnlockPreviewCommand>,
    mut preview_mode: ResMut<PreviewMode>,
) {
    if let Some(Ok(_command)) = input.take() {
        let parcel = preview_mode.preview_parcel.take();

        if let Some(parcel) = parcel {
            input.reply_ok(format!("unlocked preview to parcel {}", parcel));
        } else {
            input.reply("Preview was not locked to a parcel.");
        }
    }
}

// set thread count
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_threads")]
pub struct SceneThreadsCommand {
    threads: Option<usize>,
}

pub fn scene_threads(
    mut input: ConsoleCommand<SceneThreadsCommand>,
    mut config: ResMut<AppConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        let threads = command.threads.unwrap_or(4);
        config.scene_threads = threads;
        input.reply_ok("scene simultaneous thread count set to {threads}");
    }
}

// set fps
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/fps")]
pub struct FpsCommand {
    fps: usize,
}

pub fn set_fps(mut input: ConsoleCommand<FpsCommand>, mut config: ResMut<AppConfig>) {
    if let Some(Ok(command)) = input.take() {
        let fps = command.fps;
        config.graphics.fps_target = fps;
        input.reply_ok("target frame rate set to {fps}");
    }
}
