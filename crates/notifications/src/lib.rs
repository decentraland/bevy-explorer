pub mod plugin;

use bevy::prelude::*;

#[derive(Component)]
pub struct Notification {
    title: String,
    icon: Option<String>,
    body: Option<String>,
}

#[derive(Component, Deref, DerefMut)]
pub struct NotificationTimeout(Timer);

#[derive(Event)]
pub struct PushNotification {
    pub title: String,
    #[cfg_attr(
        not(target_os = "linux"),
        expect(dead_code, reason = "Only works on linux and web")
    )]
    pub icon: Option<String>,
    pub body: Option<String>,
    /// Timeout in seconds
    pub timeout: f32,
}
