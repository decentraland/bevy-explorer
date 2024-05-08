use std::collections::{btree_map::Entry, BTreeMap};

use bevy::prelude::*;
use common::structs::ToolTips;
use ui_core::{ui_builder::SpawnSpacer, HOVER_TEXT_STYLE};

#[derive(Component)]
pub struct ToolTipNode;

pub struct ToolTipPlugin;

impl Plugin for ToolTipPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ToolTips>();
        app.add_systems(Update, update_tooltip);
    }
}

#[allow(clippy::type_complexity)]
pub fn update_tooltip(
    windows: Query<&Window>,
    mut commands: Commands,
    mut tips: ResMut<ToolTips>,
    cur_tips: Query<Entity, With<ToolTipNode>>,
    mut active_tips: Local<BTreeMap<&'static str, (Vec<(String, bool)>, f32)>>,
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

    tips.0.retain(|_, content| !content.is_empty());

    active_tips.retain(|key, (_, vis)| {
        if tips.0.contains_key(key) {
            *vis = (*vis + time.delta_seconds() * 5.0).clamp(0.0, 1.0);
            true
        } else {
            *vis = (*vis - time.delta_seconds() * 10.0).clamp(0.0, 1.0);
            *vis > 0.0
        }
    });

    for (key, content) in tips.0.iter() {
        match active_tips.entry(key) {
            Entry::Occupied(mut o) => {
                o.get_mut().0.clone_from(content);
            }
            Entry::Vacant(v) => {
                v.insert((content.clone(), 0.0));
            }
        }
    }

    // remove any existing nodes
    for existing in cur_tips.iter() {
        commands.entity(existing).despawn_recursive();
    }

    if active_tips.is_empty() {
        return;
    }

    let mut y_offset = 0.0;

    let (left, right) = if cursor_position.x > window.width() / 2.0 {
        (
            Val::Auto,
            Val::Px(window.width() - cursor_position.x + 20.0),
        )
    } else {
        (Val::Px(cursor_position.x + 20.0), Val::Auto)
    };

    for (content, vis) in active_tips.values() {
        let columns = content[0].0.split(':').count();
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        left,
                        right,
                        top: Val::Px(
                            (cursor_position.y - content.len() as f32 * 15.0).max(0.0) + y_offset,
                        ),
                        border: UiRect::all(Val::Px(1.0)),
                        padding: UiRect::all(Val::Px(2.0)),
                        ..Default::default()
                    },
                    border_color: Color::rgba(1.0, 1.0, 1.0, 1.0 * *vis).into(),
                    background_color: Color::rgba(0.0, 0.0, 0.0, 0.5 * *vis).into(),
                    ..Default::default()
                },
                ToolTipNode,
            ))
            .with_children(|c| {
                for i in 0..columns {
                    c.spawn(NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Column,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|c| {
                        for (text, active) in content.iter() {
                            let hover_index =
                                (*vis * 9.0 * if *active { 1.0 } else { 0.3 }) as usize;
                            c.spawn(TextBundle::from_section(
                                text.split('\t').nth(i).unwrap_or_default(),
                                HOVER_TEXT_STYLE.get().unwrap()[hover_index].clone(),
                            ));
                        }
                    });

                    c.spacer();
                }
            });

        y_offset += content.len() as f32 * 30.0 + 15.0;
    }
}
