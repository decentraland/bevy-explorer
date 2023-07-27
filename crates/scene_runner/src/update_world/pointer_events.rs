use bevy::prelude::*;
use input_manager::InputMap;

use crate::update_scene::pointer_results::PointerTarget;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{common::InputAction, PbPointerEvents},
    SceneComponentId,
};
use ui_core::{ui_builder::SpawnSpacer, HOVER_TEXT_STYLE};

use super::AddCrdtInterfaceExt;

pub struct PointerEventsPlugin;

impl Plugin for PointerEventsPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbPointerEvents, PointerEvents>(
            SceneComponentId::POINTER_EVENTS,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, hover_text);
    }
}

#[derive(Component, Debug)]
pub struct PointerEvents {
    pub msg: PbPointerEvents,
}

impl From<PbPointerEvents> for PointerEvents {
    fn from(pb_pointer_events: PbPointerEvents) -> Self {
        Self {
            msg: pb_pointer_events,
        }
    }
}

#[derive(Component)]
pub struct HoverText;

#[allow(clippy::too_many_arguments)]
fn hover_text(
    mut commands: Commands,
    pointer_events: Query<&PointerEvents>,
    cur_text: Query<Entity, With<HoverText>>,
    hover_target: Res<PointerTarget>,
    windows: Query<&Window>,
    input_map: Res<InputMap>,
    mut prev_texts: Local<Vec<(InputAction, String, bool)>>,
    mut vis: Local<f32>,
    time: Res<Time>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let cursor_position = if window.cursor.grab_mode == bevy::window::CursorGrabMode::Locked {
        // if pointer locked, just middle
        Vec2::new(window.width(), window.height()) / 2.0
    } else {
        let Some(cursor_position) = window.cursor_position() else {
            // outside window
            return;
        };
        cursor_position
    };

    let mut texts = Vec::default();

    if let PointerTarget::Some {
        container,
        distance,
        ..
    } = *hover_target
    {
        if let Ok(pes) = pointer_events.get(container) {
            texts = pes
                .msg
                .pointer_events
                .iter()
                .flat_map(|pe| {
                    if let Some(info) = pe.event_info.as_ref() {
                        if info.show_feedback.unwrap_or(true) {
                            if let Some(text) = info.hover_text.as_ref() {
                                return Some((
                                    info.button(),
                                    text.to_owned(),
                                    info.max_distance.unwrap_or(10.0) > distance.0,
                                ));
                            }
                        }
                    }
                    None
                })
                .collect::<Vec<_>>();
        }
    }

    if *prev_texts != texts || texts.is_empty() {
        *vis = (*vis - time.delta_seconds() * 10.0).clamp(0.0, 1.0)
    } else {
        *vis = (*vis + time.delta_seconds() * 5.0).clamp(0.0, 1.0)
    }
    if !texts.is_empty() {
        *prev_texts = texts.clone();
    } else {
        texts = prev_texts.clone();
    }

    // remove any existing
    for existing in cur_text.iter() {
        commands.entity(existing).despawn_recursive();
    }
    if *vis > 0.0 {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left: Val::Px(cursor_position.x + 20.0),
                        top: Val::Px((cursor_position.y - texts.len() as f32 * 15.0).max(0.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        padding: UiRect::all(Val::Px(2.0)),
                        ..Default::default()
                    },
                    border_color: Color::rgba(1.0, 1.0, 1.0, 1.0 * *vis).into(),
                    background_color: Color::rgba(0.0, 0.0, 0.0, 0.5 * *vis).into(),
                    ..Default::default()
                },
                HoverText,
            ))
            .with_children(|c| {
                c.spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(|c| {
                    for (button, _, in_range) in &texts {
                        let hover_index = (*vis * 9.0 * if *in_range { 1.0 } else { 0.3 }) as usize;
                        c.spawn(TextBundle::from_section(
                            format!("{}", input_map.get_input(*button)),
                            HOVER_TEXT_STYLE.get().unwrap()[hover_index].clone(),
                        ));
                    }
                });

                c.spacer();

                c.spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(|c| {
                    for (_, _, in_range) in &texts {
                        let hover_index = (*vis * 9.0 * if *in_range { 1.0 } else { 0.3 }) as usize;
                        c.spawn(TextBundle::from_section(
                            " : ",
                            HOVER_TEXT_STYLE.get().unwrap()[hover_index].clone(),
                        ));
                    }
                });

                c.spacer();

                c.spawn(NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .with_children(|c| {
                    for (_, text, in_range) in &texts {
                        let hover_index = (*vis * 9.0 * if *in_range { 1.0 } else { 0.3 }) as usize;
                        c.spawn(TextBundle::from_section(
                            text.as_str(),
                            HOVER_TEXT_STYLE.get().unwrap()[hover_index].clone(),
                        ));
                    }
                });
            });
    }
}
