pub mod plugin;

use bevy::prelude::*;

#[derive(Component)]
pub struct Notification {
    title: String,
    icon: Option<String>,
}

#[derive(Component, Deref, DerefMut)]
pub struct NotificationTimeout(Timer);

#[derive(Event)]
pub struct PushNotification {
    pub title: String,
    pub icon: Option<String>,
    /// Timeout in seconds
    pub timeout: f32,
}
