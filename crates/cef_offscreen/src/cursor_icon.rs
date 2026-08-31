use async_channel::{Receiver, Sender};
use bevy::prelude::*;
use bevy::window::SystemCursorIcon;
use bevy::winit::cursor::CursorIcon;

/// This plugin manages the system cursor icon by receiving updates from CEF and applying them to the application window's cursor icon.
pub(super) struct SystemCursorIconPlugin;

impl Plugin for SystemCursorIconPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = async_channel::unbounded();
        app.insert_resource(SystemCursorIconSender(tx))
            .insert_resource(SystemCursorIconReceiver(rx))
            .add_systems(Update, update_cursor_icon);
    }
}

#[derive(Resource, Debug, Deref)]
pub(crate) struct SystemCursorIconSender(Sender<SystemCursorIcon>);

#[derive(Resource, Debug)]
pub(crate) struct SystemCursorIconReceiver(pub(crate) Receiver<SystemCursorIcon>);

// The original queried `Query<Entity>` — every entity in the world — and `insert`ed on each:
// fine in a 3-entity example, but in a real app it panics the moment any entity is despawned the
// same frame (and sprays CursorIcon components everywhere). Filter to actual windows and
// `try_insert` (matching upstream's later fix) so a despawn can't panic.
fn update_cursor_icon(
    mut commands: Commands,
    cursor_icon_receiver: Res<SystemCursorIconReceiver>,
    windows: Query<Entity, With<Window>>,
) {
    while let Ok(cursor_icon) = cursor_icon_receiver.0.try_recv() {
        for window in windows.iter() {
            commands
                .entity(window)
                .try_insert(CursorIcon::System(cursor_icon));
        }
    }
}
