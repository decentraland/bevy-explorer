use bevy::prelude::*;
use common::{
    inputs::SystemAction,
    sets::SetupSets,
    structs::{SystemAudio, ToolTips, TooltipSource},
    util::{FireEventEx, TryPushChildrenEx},
};
use comms::{global_crdt::MicState, Transport, TransportType};
use input_manager::{InputManager, InputPriority};
use ui_core::ui_actions::{Click, HoverEnter, HoverExit, On};

use crate::{chat::BUTTON_SCALE, SystemUiRoot};

pub struct MicUiPlugin;

#[derive(Component)]
pub struct MicUiMarker;

#[derive(Resource)]
pub struct MicImages {
    inactive: Handle<Image>,
    on: Handle<Image>,
    off: Handle<Image>,
}

impl Plugin for MicUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup.in_set(SetupSets::Main));
        app.add_systems(Update, update_mic_ui);

        let asset_server = app.world().resource::<AssetServer>();
        app.insert_resource(MicImages {
            inactive: asset_server.load("images/mic_button_inactive.png"),
            on: asset_server.load("images/mic_button_on.png"),
            off: asset_server.load("images/mic_button_off.png"),
        });
    }
}

fn setup(mut commands: Commands, images: Res<MicImages>, ui_root: Res<SystemUiRoot>) {
    // profile button
    let mic_button = commands
        .spawn((
            ImageBundle {
                image: images.inactive.clone_weak().into(),
                style: Style {
                    position_type: PositionType::Absolute,
                    top: Val::VMin(BUTTON_SCALE * 1.5),
                    right: Val::VMin(BUTTON_SCALE * 0.5),
                    width: Val::VMin(BUTTON_SCALE),
                    height: Val::VMin(BUTTON_SCALE),
                    ..Default::default()
                },
                focus_policy: bevy::ui::FocusPolicy::Block,
                ..Default::default()
            },
            Interaction::default(),
            On::<Click>::new(|mut commands: Commands, mut mic_state: ResMut<MicState>| {
                mic_state.enabled = !mic_state.enabled;
                if mic_state.enabled {
                    commands.fire_event(SystemAudio("sounds/ui/voice_chat_mic_on.wav".to_owned()));
                } else {
                    commands.fire_event(SystemAudio("sounds/ui/voice_chat_mic_off.wav".to_owned()));
                }
            }),
            On::<HoverEnter>::new(
                |mut tooltip: ResMut<ToolTips>,
                 transport: Query<&Transport>,
                 state: Res<MicState>| {
                    let transport_available = transport
                        .iter()
                        .any(|t| t.transport_type == TransportType::Livekit);
                    tooltip.0.insert(
                        TooltipSource::Label("mic"),
                        vec![(
                            "LCtrl : Push to talk".to_owned(),
                            transport_available && state.available,
                        )],
                    );
                },
            ),
            On::<HoverExit>::new(|mut tooltip: ResMut<ToolTips>| {
                tooltip.0.remove(&TooltipSource::Label("mic"));
            }),
            MicUiMarker,
        ))
        .id();

    commands.entity(ui_root.0).try_push_children(&[mic_button]);
}

#[allow(clippy::too_many_arguments)]
fn update_mic_ui(
    mut commands: Commands,
    mut mic_state: ResMut<MicState>,
    transport: Query<&Transport>,
    mut button: Query<&mut UiImage, With<MicUiMarker>>,
    mut pressed: Local<bool>,
    input_manager: InputManager,
    mic_images: Res<MicImages>,
    mut prev_active: Local<bool>,
) {
    let mic_available = mic_state.available;
    let transport_available = transport
        .iter()
        .any(|t| t.transport_type == TransportType::Livekit);

    if mic_available && transport_available {
        if mic_state.enabled {
            *button.single_mut() = mic_images.on.clone_weak().into();
        } else {
            *button.single_mut() = mic_images.off.clone_weak().into();
        }
    } else {
        *button.single_mut() = mic_images.inactive.clone_weak().into();
    }

    if input_manager.is_down(SystemAction::Microphone, InputPriority::None) != *pressed {
        *pressed = !*pressed;
        mic_state.enabled = !mic_state.enabled;
    }

    let active = mic_available && mic_state.enabled && transport_available;
    if active != *prev_active {
        if active {
            commands.fire_event(SystemAudio("sounds/ui/voice_chat_mic_on.wav".to_owned()));
        } else {
            commands.fire_event(SystemAudio("sounds/ui/voice_chat_mic_off.wav".to_owned()));
        }
        *prev_active = active;
    }
}
