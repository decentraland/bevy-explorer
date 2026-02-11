pub mod plugin;

use bevy::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::convert::{FromWasmAbi, IntoWasmAbi};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Component)]
pub struct Notification;

#[cfg(target_arch = "wasm32")]
#[derive(Component)]
pub struct Notification(<web_sys::Notification as IntoWasmAbi>::Abi);

#[cfg(target_arch = "wasm32")]
impl Drop for Notification {
    fn drop(&mut self) {
        let notification = unsafe { web_sys::Notification::from_abi(self.0) };
        notification.close();
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct NotificationTimeout(Timer);

#[derive(Event)]
pub struct PushNotification {
    pub title: String,
    /// Timeout in seconds
    pub timeout: f32,
}
