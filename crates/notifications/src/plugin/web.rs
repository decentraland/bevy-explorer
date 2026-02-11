use std::time::Duration;

use bevy::{prelude::*, time::common_conditions::on_timer};
use wasm_bindgen::convert::{FromWasmAbi, IntoWasmAbi};
use web_sys::NotificationPermission;

use crate::{plugin::NotificationsState, Notification, PushNotification};

pub struct NativeNotificationsPlugin;

impl Plugin for NativeNotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                poll_notifications_state.run_if(on_timer(Duration::from_secs(1))),
                request_permission.run_if(
                    in_state(NotificationsState::Default).and(on_event::<PushNotification>),
                ),
                build_native_notification.run_if(in_state(NotificationsState::Granted)),
            ),
        );
    }
}

#[derive(Component)]
struct NativeNotification(<web_sys::Notification as IntoWasmAbi>::Abi);

impl Drop for NativeNotification {
    fn drop(&mut self) {
        let notification = unsafe { web_sys::Notification::from_abi(self.0) };
        notification.close();
    }
}

fn poll_notifications_state(
    mut commands: Commands,
    notifications_state: Res<State<NotificationsState>>,
) {
    match web_sys::Notification::permission() {
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

fn build_native_notification(
    mut commands: Commands,
    notifications: Populated<(Entity, &Notification), Without<NativeNotification>>,
) {
    for (entity, notification) in notifications.into_inner() {
        let Ok(notification) =
            web_sys::Notification::new(&notification.title).inspect_err(|err| error!("{err:?}"))
        else {
            continue;
        };

        commands
            .entity(entity)
            .insert(NativeNotification(notification.into_abi()));
    }
}
