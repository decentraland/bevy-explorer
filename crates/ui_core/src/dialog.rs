use bevy::{ecs::system::EntityCommands, prelude::*, ui::FocusPolicy};

use crate::{BUTTON_DISABLED_TEXT_STYLE, BUTTON_TEXT_STYLE};

use super::{
    ui_actions::{Click, On},
    ui_builder::SpawnSpacer,
    BODY_TEXT_STYLE, TITLE_TEXT_STYLE,
};

pub trait SpawnDialog {
    fn spawn_dialog<M, B: IntoDialogBody>(
        &mut self,
        title: String,
        body: B,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
    ) -> Entity;

    fn spawn_dialog_two<M, N, B: IntoDialogBody>(
        &mut self,
        title: String,
        body: B,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
        button_two_label: impl Into<String>,
        button_two_action: impl IntoSystem<(), (), N>,
    ) -> Entity;
}

impl<'w, 's> SpawnDialog for Commands<'w, 's> {
    fn spawn_dialog<M, B: IntoDialogBody>(
        &mut self,
        title: String,
        body: B,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
    ) -> Entity {
        let mut dialog_inner = None;
        let dialog = self
            .spawn((NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    position_type: PositionType::Absolute,
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                focus_policy: FocusPolicy::Block,
                z_index: ZIndex::Global(5),
                ..Default::default()
            },))
            .with_children(|commands| {
                commands.spacer();
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::SpaceBetween,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spacer();
                        dialog_inner = Some(
                            commands
                                .spawn(NodeBundle {
                                    style: Style {
                                        flex_direction: FlexDirection::Column,
                                        align_content: AlignContent::Center,
                                        border: UiRect::all(Val::Px(10.0)),
                                        ..Default::default()
                                    },
                                    background_color: Color::rgb(0.6, 0.6, 0.8).into(),
                                    focus_policy: FocusPolicy::Block,
                                    ..Default::default()
                                })
                                .id(),
                        );
                        commands.spacer();
                    });
                commands.spacer();
            })
            .id();

        self.entity(dialog_inner.unwrap())
            .with_children(|commands| {
                commands.spawn(
                    TextBundle::from_section(title, TITLE_TEXT_STYLE.get().unwrap().clone())
                        .with_text_alignment(TextAlignment::Center),
                );
                body.body(commands);
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            justify_content: JustifyContent::SpaceAround,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spawn_empty().spawn_button(
                            button_one_label,
                            (move |mut commands: Commands| {
                                commands.entity(dialog).despawn_recursive()
                            })
                            .pipe(button_one_action),
                        );
                    });
            });

        dialog
    }

    fn spawn_dialog_two<M, N, B: IntoDialogBody>(
        &mut self,
        title: String,
        body: B,
        button_one_label: impl Into<String>,
        button_one_action: impl IntoSystem<(), (), M>,
        button_two_label: impl Into<String>,
        button_two_action: impl IntoSystem<(), (), N>,
    ) -> Entity {
        let mut dialog_inner = None;
        let dialog = self
            .spawn((NodeBundle {
                style: Style {
                    flex_direction: FlexDirection::Row,
                    position_type: PositionType::Absolute,
                    align_self: AlignSelf::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..Default::default()
                },
                focus_policy: FocusPolicy::Block,
                z_index: ZIndex::Global(5),
                ..Default::default()
            },))
            .with_children(|commands| {
                commands.spacer();
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            flex_direction: FlexDirection::Column,
                            justify_content: JustifyContent::SpaceBetween,
                            // size: Size::all(Val::Percent(100.0)),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spacer();
                        dialog_inner = Some(
                            commands
                                .spawn(NodeBundle {
                                    style: Style {
                                        flex_direction: FlexDirection::Column,
                                        align_content: AlignContent::Center,
                                        border: UiRect::all(Val::Px(10.0)),
                                        ..Default::default()
                                    },
                                    background_color: Color::rgb(0.6, 0.6, 0.8).into(),
                                    focus_policy: FocusPolicy::Block,
                                    ..Default::default()
                                })
                                .id(),
                        );
                        commands.spacer();
                    });
                commands.spacer();
            })
            .id();

        self.entity(dialog_inner.unwrap())
            .with_children(|commands| {
                commands.spawn(
                    TextBundle::from_section(title, TITLE_TEXT_STYLE.get().unwrap().clone())
                        .with_text_alignment(TextAlignment::Center),
                );
                body.body(commands);
                commands
                    .spawn(NodeBundle {
                        style: Style {
                            justify_content: JustifyContent::SpaceAround,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .with_children(|commands| {
                        commands.spawn_empty().spawn_button(
                            button_one_label,
                            (move |mut commands: Commands| {
                                commands.entity(dialog).despawn_recursive()
                            })
                            .pipe(button_one_action),
                        );
                        commands.spawn_empty().spawn_button(
                            button_two_label,
                            (move |mut commands: Commands| {
                                commands.entity(dialog).despawn_recursive()
                            })
                            .pipe(button_two_action),
                        );
                    });
            });

        dialog
    }
}

pub trait IntoDialogBody {
    fn body(self, commands: &mut ChildBuilder);
}

impl<T: Into<String>> IntoDialogBody for T {
    fn body(self, commands: &mut ChildBuilder) {
        commands.spawn(
            TextBundle::from_section(self, BODY_TEXT_STYLE.get().unwrap().clone())
                .with_text_alignment(TextAlignment::Center),
        );
    }
}

pub struct TitleText<T: Into<String>>(pub T);

impl<T: Into<String>> IntoDialogBody for TitleText<T> {
    fn body(self, commands: &mut ChildBuilder) {
        commands.spawn(
            TextBundle::from_section(self.0, TITLE_TEXT_STYLE.get().unwrap().clone())
                .with_text_alignment(TextAlignment::Center),
        );
    }
}

pub struct ButtonText<T: Into<String>>(pub T);

impl<T: Into<String>> IntoDialogBody for ButtonText<T> {
    fn body(self, commands: &mut ChildBuilder) {
        commands.spawn(
            TextBundle::from_section(self.0, BUTTON_TEXT_STYLE.get().unwrap().clone())
                .with_text_alignment(TextAlignment::Center),
        );
    }
}

pub struct ButtonDisabledText<T: Into<String>>(pub T);

impl<T: Into<String>> IntoDialogBody for ButtonDisabledText<T> {
    fn body(self, commands: &mut ChildBuilder) {
        commands.spawn(
            TextBundle::from_section(self.0, BUTTON_DISABLED_TEXT_STYLE.get().unwrap().clone())
                .with_text_alignment(TextAlignment::Center),
        );
    }
}

pub trait SpawnButton {
    fn spawn_button<B: IntoDialogBody, M>(
        &mut self,
        text: B,
        button_action: impl IntoSystem<(), (), M>,
    ) -> &mut Self;
}

impl<'w, 's, 'a> SpawnButton for EntityCommands<'w, 's, 'a> {
    fn spawn_button<B: IntoDialogBody, M>(
        &mut self,
        body: B,
        button_action: impl IntoSystem<(), (), M>,
    ) -> &mut Self {
        self.insert((
            NodeBundle {
                style: Style {
                    border: UiRect::all(Val::Px(1.0)),
                    margin: UiRect::all(Val::Px(2.0)),
                    ..Default::default()
                },
                border_color: Color::rgb(0.2, 0.2, 0.0).into(),
                background_color: Color::rgb(0.8, 0.8, 1.0).into(),
                ..Default::default()
            },
            Interaction::default(),
            On::<Click>::new(button_action),
        ))
        .with_children(|c| {
            body.body(c);
        })
    }
}
