use avatar::animate::EmoteList;
use bevy::{
    prelude::*,
    text::BreakLineOn,
    ui::FocusPolicy,
    utils::HashSet,
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiComponentFromClone, DuiEntityCommandsExt, DuiProps, DuiRegistry};
use common::structs::PrimaryUser;
use comms::profile::CurrentUserProfile;
use emotes::AvatarAnimations;
use ui_core::{
    focus::Focus,
    ui_actions::{Click, Defocus, HoverEnter, HoverExit, On},
    ModifyComponentExt,
};

use crate::chat::BUTTON_SCALE;

pub struct EmoteUiPlugin;

impl Plugin for EmoteUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<EmoteUiEvent>()
            .add_systems(Startup, setup)
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

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut dui: ResMut<DuiRegistry>) {
    // emote button
    commands.spawn((
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
    ));

    dui.register_template(
        "popup-layout",
        DuiComponentFromClone::<DuiLayout>::new("layout"),
    );
}

#[allow(clippy::too_many_arguments)]
fn handle_emote_key(
    mut commands: Commands,
    player: Query<Entity, With<PrimaryUser>>,
    key_input: Res<ButtonInput<KeyCode>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut w: EventWriter<EmoteUiEvent>,
    time: Res<Time>,
    existing: Query<&EmoteDialog>,
    buttons: Query<&EmoteButton>,
    mut press_time: Local<f32>,
) {
    if key_input.just_pressed(KeyCode::AltLeft) {
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

    if key_input.just_released(KeyCode::AltLeft) && time.elapsed_seconds() > *press_time + 0.25 {
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
                    commands
                        .entity(player.single())
                        .try_insert(EmoteList::new(button.0.clone()));
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

#[derive(Event)]
pub enum EmoteUiEvent {
    Show { coords: Option<Vec2> },
    Hide,
}

fn update_dui_props(mut dui: ResMut<DuiRegistry>, window: Query<&Window, With<PrimaryWindow>>) {
    let window = window.single();
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
    let window = window.single();
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
    emotes: Res<AvatarAnimations>,
    asset_server: Res<AssetServer>,
    buttons: Query<(&EmoteButton, &Interaction)>,
    player: Query<Entity, With<PrimaryUser>>,
) {
    if let Some(ev) = events.read().last() {
        for ent in existing.iter() {
            commands.entity(ent).despawn_recursive();

            for (button, interact) in &buttons {
                if interact == &Interaction::Hovered || interact == &Interaction::Pressed {
                    commands
                        .entity(player.single())
                        .try_insert(EmoteList::new(button.0.clone()));
                }
            }
        }

        let EmoteUiEvent::Show { coords } = ev else {
            return;
        };

        let mut props = window.single().get_layout_props(1.5, 0.6, *coords);

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
                format!("image_{}", i),
                asset_server.load::<Image>("images/redx.png"),
            );
        }

        for emote in player_emotes {
            debug!("adding {}", emote.slot);
            let h_thumb = emotes
                .get_server(&emote.urn)
                .and_then(|anim| anim.thumbnail.clone())
                .unwrap_or_else(|| {
                    debug!("didn't find {} in {:?}", emote.urn, emotes);
                    asset_server.load("images/redx.png")
                });
            props.insert_prop(format!("image_{}", emote.slot), h_thumb.clone())
        }

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
        commands.entity(output).insert(EmoteOutput);

        let mut all_slots = (0..=9).collect::<HashSet<u32>>();
        for emote in player_emotes {
            all_slots.remove(&emote.slot);
            let button = buttons.named(format!("emote_{}", emote.slot).as_str());
            let name = emotes
                .get_server(&emote.urn)
                .map(|e| e.name.clone())
                .unwrap_or("???".to_owned());
            let name2 = name.clone();
            commands.entity(button).insert((
                EmoteButton(emote.urn.clone(), emote.slot),
                Interaction::default(),
                FocusPolicy::Block,
                On::<HoverEnter>::new(
                    move |mut color: Query<&mut BackgroundColor>, mut text: Query<&mut Text>| {
                        if let Ok(mut bg) = color.get_mut(button) {
                            bg.0 = Color::rgb(1.0, 1.0, 1.50);
                        }
                        if let Ok(mut text) = text.get_mut(output) {
                            text.sections[0].value = name.clone();
                            text.linebreak_behavior = BreakLineOn::WordBoundary;
                        }
                    },
                ),
                On::<HoverExit>::new(
                    move |mut color: Query<&mut BackgroundColor>, mut text: Query<&mut Text>| {
                        if let Ok(mut bg) = color.get_mut(button) {
                            bg.0 = Color::rgb(0.67, 0.67, 0.87);
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
                .modify_component(|bg: &mut BackgroundColor| bg.0 = Color::GRAY);
        }
    }
}

#[derive(Component)]
pub struct EmoteOutput;
