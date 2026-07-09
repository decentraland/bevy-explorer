use crate::components::CefWebviewUri;
use crate::core::prelude::{Browsers, create_cef_key_events, keyboard_modifiers};
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

pub(super) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, send_key_event.run_if(on_event::<KeyboardInput>));
    }
}

// All key events are forwarded to every webview (like document-level listeners in a browser);
// the page decides what to react to, and the host app decides separately what the engine ignores.
fn send_key_event(
    mut er: EventReader<KeyboardInput>,
    input: Res<ButtonInput<KeyCode>>,
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<CefWebviewUri>>,
) {
    let modifiers = keyboard_modifiers(&input);
    for event in er.read() {
        for key_event in create_cef_key_events(modifiers, event) {
            for webview in webviews.iter() {
                browsers.send_key(&webview, key_event.clone());
            }
        }
    }
}
