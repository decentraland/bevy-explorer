use std::time::Duration;

use bevy::{
    app::NonSendMarker, ecs::entity::EntityHashMap, prelude::*, time::common_conditions::on_timer,
};
use web_sys::{NotificationOptions, NotificationPermission};

use crate::{plugin::NotificationsState, Notification, PushNotification};

pub struct NativeNotificationsPlugin;

impl Plugin for NativeNotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_non_send_resource::<NativeNotifications>();

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

        app.add_observer(notification_removed);
    }
}

#[derive(Default, Deref, DerefMut)]
struct NativeNotifications(EntityHashMap<web_sys::Notification>);

#[derive(Component)]
struct NativeNotification;

fn poll_notifications_state(
    mut commands: Commands,
    notifications_state: Res<State<NotificationsState>>,
    _: NonSend<NativeNotifications>,
) {
    match web_sys::Notification::permission() {
        NotificationPermission::Default => {
            if *notifications_state.get() != NotificationsState::Default {
                debug!("NotificationState Default");
                commands.set_state(NotificationsState::Default);
            }
        }
        NotificationPermission::Denied => {
            if *notifications_state.get() != NotificationsState::Denied {
                debug!("NotificationState Denied");
                commands.set_state(NotificationsState::Denied);
            }
        }
        NotificationPermission::Granted => {
            if *notifications_state.get() != NotificationsState::Granted {
                debug!("NotificationState Granted");
                commands.set_state(NotificationsState::Granted);
            }
        }
        other => panic!("Unknown NotificationPermission {:?}.", other),
    }
}

fn request_permission(_: NonSend<NativeNotifications>) {
    debug!("Requesting notification permission");
    let _ = web_sys::Notification::request_permission().inspect_err(|err| error!("{err:?}"));
}

fn build_native_notification(
    mut commands: Commands,
    notifications: Populated<(Entity, &Notification), Without<NativeNotification>>,
    mut native_notifications: NonSendMut<NativeNotifications>,
) {
    for (entity, notification) in notifications.into_inner() {
        let options = NotificationOptions::default();
        if let Some(ref icon) = notification.icon {
            options.set_icon(icon);
        }
        if let Some(ref body) = notification.body {
            options.set_body(body);
        }
        let Ok(notification) =
            web_sys::Notification::new_with_options(&notification.title, &options)
                .inspect_err(|err| error!("{err:?}"))
        else {
            continue;
        };

        debug!("Built web notification", notification);
        commands.entity(entity).insert(NativeNotification);
        native_notifications.insert(entity, notification);
    }
}

fn notification_removed(
    trigger: Trigger<OnRemove, NativeNotification>,
    mut notifications: NonSendMut<NativeNotifications>,
) {
    let entity = trigger.target();

    if let Some(notification) = notifications.remove(&entity) {
        debug!("Notification finished", notification);
        notification.close();
    }
}
