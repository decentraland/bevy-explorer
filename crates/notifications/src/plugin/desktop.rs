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
        let mut notify = notify_rust::Notification::new();
        notify.summary(&notification.title);
        #[cfg(target_os = "linux")]
        if let Some(ref icon) = notification.icon {
            notify.icon(icon);
        }
        if let Some(ref body) = notification.body {
            notify.body(body);
        }

        let Ok(notification_handle) = notify.show().inspect_err(|err| error!("{err:?}")) else {
            continue;
        };

        commands
            .entity(entity)
            .insert(NativeNotification(notification_handle));
    }
}
