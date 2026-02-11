#[cfg(target_arch = "wasm32")]
mod web;

use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::convert::IntoWasmAbi;

use crate::{Notification, NotificationTimeout, PushNotification};

pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<NotificationsState>();
        app.add_event::<PushNotification>();

        #[cfg(target_arch = "wasm32")]
        app.add_plugins(web::WebNotificationsPlugin);

        app.add_systems(Update, (tick_notifications, notification_pushed));
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
            },
            NotificationTimeout(Timer::from_seconds(
                push_notification.timeout,
                TimerMode::Once,
            )),
        ));
    }
}
