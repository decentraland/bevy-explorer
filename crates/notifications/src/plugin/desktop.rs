use bevy::prelude::*;
use notify_rust::NotificationHandle;

use crate::{plugin::NotificationsState, Notification};

pub struct NativeNotificationsPlugin;

impl Plugin for NativeNotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_state(NotificationsState::Granted);

        app.add_systems(
            Update,
            build_native_notification.run_if(in_state(NotificationsState::Granted)),
        );
    }
}

#[expect(dead_code, reason = "Might be usable later")]
#[derive(Component)]
struct NativeNotification(NotificationHandle);

fn build_native_notification(
    mut commands: Commands,
    notifications: Populated<(Entity, &Notification), Without<NativeNotification>>,
) {
    for (entity, notification) in notifications.into_inner() {
        let Ok(notification_handle) = notify_rust::Notification::new()
            .summary(&notification.title)
            .show()
            .inspect_err(|err| error!("{err:?}"))
        else {
            continue;
        };

        commands
            .entity(entity)
            .insert(NativeNotification(notification_handle));
    }
}
