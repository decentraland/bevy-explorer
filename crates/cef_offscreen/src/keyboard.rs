use crate::components::CefWebviewUri;
use crate::core::prelude::{Browsers, create_cef_key_events, keyboard_modifiers};
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::prelude::*;

pub(super) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrackedModifiers>().add_systems(
            Update,
            send_key_event.run_if(on_event::<KeyboardInput>.or(on_event::<KeyboardFocusLost>)),
        );
    }
}

/// Modifier state tracked from the raw event stream rather than `ButtonInput<KeyCode>`: the host
/// app may scrub non-suppressing modifiers (e.g. Shift) out of `ButtonInput` while an OS-shortcut
/// chord is held (bevy-explorer's input_manager does), which would strip shiftKey from forwarded
/// chords like Cmd+Shift+F. Events are delivered regardless, so this stays accurate; it resets on
/// focus loss (where keyups can go missing).
#[derive(Resource, Default)]
struct TrackedModifiers(bevy::platform::collections::HashSet<KeyCode>);

// All key events are forwarded to every webview (like document-level listeners in a browser);
// the page decides what to react to, and the host app decides separately what the engine ignores.
fn send_key_event(
    mut er: EventReader<KeyboardInput>,
    mut focus_lost: EventReader<KeyboardFocusLost>,
    mut tracked: ResMut<TrackedModifiers>,
    input: Res<ButtonInput<KeyCode>>,
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<CefWebviewUri>>,
) {
    if focus_lost.read().next().is_some() {
        tracked.0.clear();
    }
    for event in er.read() {
        if matches!(
            event.key_code,
            KeyCode::ShiftLeft
                | KeyCode::ShiftRight
                | KeyCode::ControlLeft
                | KeyCode::ControlRight
                | KeyCode::AltLeft
                | KeyCode::AltRight
                | KeyCode::SuperLeft
                | KeyCode::SuperRight
        ) {
            match event.state {
                bevy::input::ButtonState::Pressed => tracked.0.insert(event.key_code),
                bevy::input::ButtonState::Released => tracked.0.remove(&event.key_code),
            };
        }
        // union of both sources: events survive host-side ButtonInput scrubbing, ButtonInput
        // covers state from before this webview existed
        let modifiers = keyboard_modifiers(&input, &tracked.0);
        for key_event in create_cef_key_events(modifiers, event) {
            for webview in webviews.iter() {
                browsers.send_key(&webview, key_event.clone());
            }
        }
    }
}
