use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use web_sys::{Notification, NotificationPermission};

use crate::{plugin::NotificationsState, PushNotification};

pub struct WebNotificationsPlugin;

impl Plugin for WebNotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                poll_notifications_state.run_if(on_timer(Duration::from_secs(1))),
                request_permission.run_if(
                    in_state(NotificationsState::Default).and(on_event::<PushNotification>),
                ),
            ),
        );
    }
}

fn poll_notifications_state(
    mut commands: Commands,
    notifications_state: Res<State<NotificationsState>>,
) {
    match Notification::permission() {
        NotificationPermission::Default => {
            if *notifications_state.get() != NotificationsState::Default {
                commands.set_state(NotificationsState::Default);
            }
        }
        NotificationPermission::Denied => {
            if *notifications_state.get() != NotificationsState::Denied {
                commands.set_state(NotificationsState::Denied);
            }
        }
        NotificationPermission::Granted => {
            if *notifications_state.get() != NotificationsState::Granted {
                commands.set_state(NotificationsState::Granted);
            }
        }
        other => panic!("Unknown NotificationPermission {:?}.", other),
    }
}

fn request_permission() {
    let _ = web_sys::Notification::request_permission().inspect_err(|err| error!("{err:?}"));
}
