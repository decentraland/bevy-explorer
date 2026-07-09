use crate::components::CefWebviewUri;
use crate::core::prelude::{Browsers, create_cef_key_event, keyboard_modifiers};
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

/// Which input the host app currently wants delivered to CEF. The app is expected to drive this
/// per frame (e.g. from a cursor-over-HUD hit test + page text-focus signal); keys are dropped
/// while `keyboard` is false so world-control keys don't trigger page hotkeys.
#[derive(Resource, Debug)]
pub struct CefInputGate {
    pub keyboard: bool,
}

impl Default for CefInputGate {
    fn default() -> Self {
        Self { keyboard: true }
    }
}

pub(super) struct KeyboardPlugin;

impl Plugin for KeyboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CefInputGate>()
            .add_systems(Update, send_key_event.run_if(on_event::<KeyboardInput>));
    }
}

fn send_key_event(
    mut er: EventReader<KeyboardInput>,
    gate: Res<CefInputGate>,
    input: Res<ButtonInput<KeyCode>>,
    browsers: NonSend<Browsers>,
    webviews: Query<Entity, With<CefWebviewUri>>,
) {
    if !gate.keyboard {
        er.clear();
        return;
    }
    let modifiers = keyboard_modifiers(&input);
    for event in er.read() {
        let Some(key_event) = create_cef_key_event(modifiers, &input, event) else {
            continue;
        };
        for webview in webviews.iter() {
            browsers.send_key(&webview, key_event.clone());
        }
    }
}
