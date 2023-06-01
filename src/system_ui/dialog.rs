use bevy::{prelude::*, ui::FocusPolicy};

use super::{ui_actions::{On, Click}, BODY_TEXT_STYLE, BUTTON_TEXT_STYLE, TITLE_TEXT_STYLE};

pub trait SpawnDialog {
    fn spawn_dialog_two<M, N>(
        &mut self,
        title: String,
        body: String,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
        button_two_label: impl Into<String>,
        button_two_action: impl IntoSystem<(), (), N>,
    );
}

impl<'w, 's> SpawnDialog for Commands<'w, 's> {
    fn spawn_dialog_two<M, N>(
        &mut self,
        title: String,
        body: String,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
        button_two_label: impl Into<String>,
        button_two_action: impl IntoSystem<(), (), N>,
    ) {
        let mut dialog_inner = None;
        let dialog = self
            .spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Row,
                        position_type: PositionType::Absolute,
                        align_self: AlignSelf::Center,
                        justify_content: JustifyContent::SpaceBetween,
                        size: Size::width(Val::Percent(100.0)),
                        ..Default::default()
                    },
                    z_index: ZIndex::Global(5),
                    ..Default::default()
                },
                // Modal,
            ))
            .with_children(|commands| {
                commands.spawn(NodeBundle {
                    style: Style {
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                });
                dialog_inner = Some(
                    commands
                        .spawn(NodeBundle {
                            style: Style {
                                flex_direction: FlexDirection::Column,
                                align_content: AlignContent::Center,
                                border: UiRect::all(Val::Px(10.0)),
                                ..Default::default()
                            },
                            background_color: Color::rgb(0.8, 0.8, 0.6).into(),
                            focus_policy: FocusPolicy::Block,
                            ..Default::default()
                        })
                        .id(),
                );
                commands.spawn(NodeBundle {
                    style: Style {
                        flex_grow: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                });
            })
            .id();

        self.entity(dialog_inner.unwrap())
            .with_children(|commands| {
                commands.spawn(
                    TextBundle::from_section(title, TITLE_TEXT_STYLE.get().unwrap().clone())
                        .with_text_alignment(TextAlignment::Center),
                );
                commands.spawn(
                    TextBundle::from_section(body, BODY_TEXT_STYLE.get().unwrap().clone())
                        .with_text_alignment(TextAlignment::Center),
                );
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            justify_content: JustifyContent::SpaceAround,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spawn((
                            TextBundle {
                                style: Style {
                                    margin: UiRect::all(Val::Px(10.0)),
                                    ..Default::default()
                                },
                                text: Text::from_section(
                                    button_one_label,
                                    BUTTON_TEXT_STYLE.get().unwrap().clone(),
                                ),
                                background_color: Color::rgb(1.0, 1.0, 0.8).into(),
                                ..Default::default()
                            },
                            Interaction::default(),
                            On::<Click>::new(
                                (move |mut commands: Commands| {
                                    commands.entity(dialog).despawn_recursive()
                                })
                                .pipe(button_one_action),
                            ),
                        ));
                        commands.spawn((
                            TextBundle {
                                text: Text::from_section(
                                    button_two_label,
                                    BUTTON_TEXT_STYLE.get().unwrap().clone(),
                                ),
                                background_color: Color::rgb(0.9, 0.9, 0.5).into(),
                                ..Default::default()
                            },
                            Interaction::default(),
                            On::<Click>::new(
                                (move |mut commands: Commands| {
                                    commands.entity(dialog).despawn_recursive()
                                })
                                .pipe(button_two_action),
                            ),
                        ));
                    });
            });
    }
}
