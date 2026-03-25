#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
mod desktop;
#[cfg(target_arch = "wasm32")]
mod web;

use bevy::prelude::*;

#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use crate::plugin::desktop::NativeNotificationsPlugin;
#[cfg(target_arch = "wasm32")]
use crate::plugin::web::NativeNotificationsPlugin;
use crate::{Notification, NotificationTimeout, PushNotification};
use std::time::Duration;
use bevy::time::common_conditions::on_timer;

pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<NotificationsState>();
        app.add_event::<PushNotification>();

        app.add_plugins(NativeNotificationsPlugin);

        app.add_systems(Update, (tick_notifications, notification_pushed));

        app.add_systems(Update, spam.run_if(on_timer(Duration::from_secs(8))));
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
pub enum NotificationsState {
    #[default]
    Default,
    Denied,
    Granted,
}

fn tick_notifications(
    mut commands: Commands,
    notifications: Populated<(Entity, &mut NotificationTimeout)>,
    time: Res<Time>,
) {
    let delta = time.delta();
    for (entity, mut notification_timeout) in notifications.into_inner() {
        if notification_timeout.tick(delta).just_finished() {
            commands.entity(entity).despawn();
        }
    }
}

fn notification_pushed(
    mut commands: Commands,
    mut push_notifications: EventReader<PushNotification>,
) {
    for push_notification in push_notifications.read() {
        commands.spawn((
            Notification {
                title: push_notification.title.clone(),
                icon: push_notification.icon.clone(),
                body: push_notification.body.clone(),
            },
            NotificationTimeout(Timer::from_seconds(
                push_notification.timeout,
                TimerMode::Once,
            )),
        ));
    }
}

fn spam(mut commands: Commands) {
    commands.send_event(PushNotification {
        title: "Spam".to_owned(),
        #[cfg(not(target_arch = "wasm32"))]
        icon: Some("file://home/hukasu/.local/share/bevyexplorer/cache/Sk2HvA9W6QYIHGSOj5YBuyutJbOgNGgWg9VPpoWmpK0".to_owned()),
        #[cfg(target_arch = "wasm32")]
        icon: Some("favicon/favicon-96x96.png".to_owned()),
        body: Some("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Fusce eleifend auctor velit. Curabitur ultricies molestie finibus. Praesent nec imperdiet velit. Morbi vulputate tellus tellus, eget blandit mauris finibus a. Praesent commodo erat et metus finibus, et cursus turpis varius. Donec non nibh vel dolor pharetra ultrices. Aenean eget massa risus. Aliquam erat volutpat.".to_owned()),
        timeout: 4.,
    });
}