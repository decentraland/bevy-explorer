use bevy::{
    color::palettes::css,
    core::FrameCount,
    prelude::*,
    text::BreakLineOn,
    ui::FocusPolicy,
    platform::collections::HashSet,
    window::{PrimaryWindow, WindowFocused, WindowResized},
};
use bevy_dui::{DuiComponentFromClone, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use collectibles::{CollectibleError, CollectibleManager, Emote, EmoteUrn};
use common::{
    inputs::SystemAction,
    sets::SetupSets,
    structs::{ActiveDialog, EmoteCommand, PrimaryUser, SystemAudio},
    util::{FireEventEx, ModifyComponentExt, TryPushChildrenEx},
};
use comms::profile::CurrentUserProfile;
use input_manager::{InputManager, InputPriority};
use system_bridge::NativeUi;
use ui_core::{
    focus::Focus,
    ui_actions::{Click, Defocus, HoverEnter, HoverExit, On},
};

use crate::{chat::BUTTON_SCALE, SystemUiRoot};

pub struct EmoteUiPlugin;

impl Plugin for EmoteUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().resource::<NativeUi>().emote_wheel {
            return;
        }

        app.add_event::<EmoteUiEvent>()
            .add_systems(Startup, setup.in_set(SetupSets::Main))
            .add_systems(
                Update,
                (
                    update_dui_props,
                    handle_emote_key,
                    apply_layout,
                    show_emote_ui
                        .run_if(|profile: Res<CurrentUserProfile>| profile.profile.is_some()),
                ),
            );
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut dui: ResMut<DuiRegistry>,
    ui_root: Res<SystemUiRoot>,
) {
    // emote button
    let button = commands
        .spawn((
            ImageBundle {
                image: asset_server.load("images/emote_button.png").into(),
                style: Style {
                    position_type: PositionType::Absolute,
                    top: Val::VMin(BUTTON_SCALE * 2.5),
                    right: Val::VMin(BUTTON_SCALE * 0.5),
                    width: Val::VMin(BUTTON_SCALE),
                    height: Val::VMin(BUTTON_SCALE),
                    ..Default::default()
                },
                focus_policy: bevy::ui::FocusPolicy::Block,
                ..Default::default()
            },
            Interaction::default(),
            On::<Click>::new(
                |mut w: EventWriter<EmoteUiEvent>, existing: Query<&EmoteDialog>| {
                    if existing.is_empty() {
                        w.send(EmoteUiEvent::Show { coords: None });
                    } else {
                        w.send(EmoteUiEvent::Hide);
                    }
                },
            ),
        ))
        .id();

    dui.register_template(
        "popup-layout",
        DuiComponentFromClone::<DuiLayout>::new("layout"),
    );

    commands.entity(ui_root.0).try_push_children(&[button]);
}

#[allow(clippy::too_many_arguments)]
fn handle_emote_key(
    mut commands: Commands,
    player: Query<Entity, With<PrimaryUser>>,
    key_input: Res<ButtonInput<KeyCode>>,
    input_manager: InputManager,
    window: Query<&Window, With<PrimaryWindow>>,
    mut w: EventWriter<EmoteUiEvent>,
    time: Res<Time>,
    existing: Query<&EmoteDialog>,
    buttons: Query<&EmoteButton>,
    mut press_time: Local<f32>,
    mut lost_focus_events: EventReader<WindowFocused>,
    frame: Res<FrameCount>,
) {
    if input_manager.just_down(SystemAction::Emote, InputPriority::None) {
        if !existing.is_empty() {
            w.send(EmoteUiEvent::Hide);
            return;
        }

        let window = window.single();
        let coords = if window.cursor.grab_mode != bevy::window::CursorGrabMode::Locked {
            window.cursor_position()
        } else {
            None
        };

        w.send(EmoteUiEvent::Show { coords });
        *press_time = time.elapsed_seconds();
    }

    if input_manager.just_up(SystemAction::Emote) && time.elapsed_seconds() > *press_time + 0.25 {
        w.send(EmoteUiEvent::Hide);
    }

    if lost_focus_events.read().any(|ev| !ev.focused) {
        w.send(EmoteUiEvent::Hide);
    }

    const EMOTE_KEYS: [(KeyCode, u32); 10] = [
        (KeyCode::Digit0, 0),
        (KeyCode::Digit1, 1),
        (KeyCode::Digit2, 2),
        (KeyCode::Digit3, 3),
        (KeyCode::Digit4, 4),
        (KeyCode::Digit5, 5),
        (KeyCode::Digit6, 6),
        (KeyCode::Digit7, 7),
        (KeyCode::Digit8, 8),
        (KeyCode::Digit9, 9),
    ];
    if !existing.is_empty() {
        for (emote_key, slot) in EMOTE_KEYS {
            if key_input.just_pressed(emote_key) {
                if let Some(button) = buttons.iter().find(|b| b.1 == slot) {
                    commands.entity(player.single()).try_insert(EmoteCommand {
                        urn: button.0.clone(),
                        r#loop: false,
                        timestamp: frame.0 as i64,
                    });
                    w.send(EmoteUiEvent::Hide);
                }
            }
        }
    }
}

#[derive(Component)]
pub struct EmoteDialog;

#[derive(Component)]
pub struct EmoteButton(String, u32);

#[derive(Event, Clone, Copy)]
pub enum EmoteUiEvent {
    Show { coords: Option<Vec2> },
    Hide,
}

fn update_dui_props(mut dui: ResMut<DuiRegistry>, window: Query<&Window, With<PrimaryWindow>>) {
    let Ok(window) = window.get_single() else {
        return;
    };
    let aspect_size = window.width().min(window.height());
    dui.set_default_prop("font-large", format!("{}px", (aspect_size * 0.05) as u32));
    dui.set_default_prop("font-med", format!("{}px", (aspect_size * 0.025) as u32));
    dui.set_default_prop("font-small", format!("{}px", (aspect_size * 0.0125) as u32));
}

pub trait LayoutPropsEx {
    fn get_layout_props(&self, w_div_h: f32, base_width: f32, center: Option<Vec2>) -> DuiProps;
}

impl LayoutPropsEx for Window {
    fn get_layout_props(&self, w_div_h: f32, width: f32, center: Option<Vec2>) -> DuiProps {
        let viewport = Vec2::new(self.width(), self.height());
        let viewport_ratio = viewport.x / viewport.y;

        let size_pct = Vec2::new(
            width * (1.0 / viewport_ratio).min(1.0),
            width / w_div_h * viewport_ratio.min(1.0),
        );

        let center = (center.unwrap_or(viewport / 2.0) / viewport)
            .clamp(size_pct / 2.0, Vec2::ONE - size_pct / 2.0);
        let Vec2 { x: left, y: top } = (center - size_pct / 2.0) * 100.0;

        DuiProps::new()
            .with_prop("left", format!("{left}%"))
            .with_prop("top", format!("{top}%"))
            .with_prop("layout", DuiLayout { width, w_div_h })
    }
}

fn apply_layout(
    mut q: Query<(&mut Style, Ref<DuiLayout>)>,
    mut resized: EventReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let resized = resized.read().last().is_some();
    let Ok(window) = window.get_single() else {
        return;
    };
    let viewport = Vec2::new(window.width(), window.height());
    let viewport_ratio = viewport.x / viewport.y;

    if resized {
        debug!("resized: {viewport}");
    }

    for (mut style, layout) in q.iter_mut() {
        if layout.is_added() || layout.is_changed() || resized {
            let size_pct = Vec2::new(
                layout.width * (1.0 / viewport_ratio).min(1.0),
                layout.width / layout.w_div_h * viewport_ratio.min(1.0),
            );

            let Vec2 {
                x: width,
                y: height,
            } = size_pct * viewport;

            style.width = Val::Px(width);
            style.height = Val::Px(height);
        }
    }
}

#[derive(Component, Clone, Copy)]
pub struct DuiLayout {
    pub width: f32,
    pub w_div_h: f32,
}

#[allow(clippy::too_many_arguments)]
// panel shows until button released or any click
fn show_emote_ui(
    mut commands: Commands,
    mut events: EventReader<EmoteUiEvent>,
    existing: Query<Entity, With<EmoteDialog>>,
    dui: Res<DuiRegistry>,
    window: Query<&Window, With<PrimaryWindow>>,
    profile: Res<CurrentUserProfile>,
    mut emote_loader: CollectibleManager<Emote>,
    asset_server: Res<AssetServer>,
    buttons: Query<(&EmoteButton, &Interaction)>,
    player: Query<Entity, With<PrimaryUser>>,
    mut retry: Local<Option<EmoteUiEvent>>,
    active_dialog: Res<ActiveDialog>,
    frame: Res<FrameCount>,
) {
    let mut ev = events.read().last().copied();
    if retry.is_some() {
        ev = retry.take();
    }

    if let Some(ev) = ev {
        for ent in existing.iter() {
            commands.fire_event(SystemAudio("sounds/ui/widget_emotes_close.wav".to_owned()));
            commands.entity(ent).despawn_recursive();

            for (button, interact) in &buttons {
                if interact != &Interaction::None {
                    commands.entity(player.single()).try_insert(EmoteCommand {
                        urn: button.0.clone(),
                        r#loop: false,
                        timestamp: frame.0 as i64,
                    });
                }
            }
        }

        let Some(permit) = active_dialog.try_acquire() else {
            return;
        };

        let EmoteUiEvent::Show { coords } = ev else {
            return;
        };

        let mut props = window.single().get_layout_props(1.5, 0.6, coords);

        let Some(player_emotes) = profile
            .profile
            .as_ref()
            .and_then(|p| p.content.avatar.emotes.as_ref())
        else {
            return;
        };

        for i in 0..=9 {
            // we will remove the empty slots later
            props.insert_prop(
                format!("image_{i}"),
                asset_server.load::<Image>("images/redx.png"),
            );
        }

        let mut all_loaded = true;

        for emote in player_emotes {
            debug!("adding {}", emote.slot);

            let h_thumb = EmoteUrn::new(&emote.urn)
                .ok()
                .and_then(|emote_urn| match emote_loader.get_data(emote_urn) {
                    Ok(d) => Some(d),
                    Err(CollectibleError::Loading) => {
                        all_loaded = false;
                        None
                    }
                    _ => None,
                })
                .map(|anim| anim.thumbnail.clone())
                .unwrap_or_else(|| {
                    debug!("didn't find {}", emote.urn);
                    asset_server.load("images/redx.png")
                });
            props.insert_prop(format!("image_{}", emote.slot), h_thumb.clone())
        }

        if !all_loaded {
            *retry = Some(ev);
            return;
        }

        commands.fire_event(SystemAudio("sounds/ui/widget_emotes_open.wav".to_owned()));

        let buttons = commands
            .spawn((
                EmoteDialog,
                Focus,
                Interaction::default(),
                On::<Defocus>::new(|mut w: EventWriter<EmoteUiEvent>| {
                    w.send(EmoteUiEvent::Hide);
                }),
            ))
            .apply_template(&dui, "choose-emote-base", props)
            .unwrap();

        let output = buttons.named("output");
        commands.entity(output).insert((EmoteOutput, permit));

        let mut all_slots = (0..=9).collect::<HashSet<u32>>();
        for emote in player_emotes {
            all_slots.remove(&emote.slot);
            let button = buttons.named(format!("emote_{}", emote.slot).as_str());
            let name = EmoteUrn::new(&emote.urn)
                .ok()
                .and_then(|emote| emote_loader.get_data(emote).ok())
                .map(|e| e.name.clone())
                .unwrap_or("???".to_owned());
            let name2 = name.clone();
            commands.entity(button).insert((
                EmoteButton(emote.urn.clone(), emote.slot),
                Interaction::default(),
                FocusPolicy::Block,
                On::<HoverEnter>::new(
                    move |mut commands: Commands,
                          mut color: Query<&mut UiImage>,
                          mut text: Query<&mut Text>| {
                        commands.fire_event(SystemAudio(
                            "sounds/ui/widget_emotes_highlight.wav".to_owned(),
                        ));
                        if let Ok(mut img) = color.get_mut(button) {
                            img.color = Color::srgb(1.0, 1.0, 1.50);
                        }
                        if let Ok(mut text) = text.get_mut(output) {
                            text.sections[0].value.clone_from(&name);
                            text.linebreak_behavior = BreakLineOn::WordBoundary;
                        }
                    },
                ),
                On::<HoverExit>::new(
                    move |mut color: Query<&mut UiImage>, mut text: Query<&mut Text>| {
                        if let Ok(mut img) = color.get_mut(button) {
                            img.color = Color::srgb(0.67, 0.67, 0.87);
                        }
                        if let Ok(mut text) = text.get_mut(output) {
                            if text.sections[0].value == name2 {
                                text.sections[0].value = String::default();
                            }
                        }
                    },
                ),
            ));
        }

        for unused_slot in all_slots {
            commands
                .entity(buttons.named(format!("image_{unused_slot}").as_str()))
                .despawn_recursive();
            commands
                .entity(buttons.named(format!("emote_{unused_slot}").as_str()))
                .modify_component(|img: &mut UiImage| img.color = css::GRAY.into());
        }
    }
}

#[derive(Component)]
pub struct EmoteOutput;
