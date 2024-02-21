use bevy::{ecs::system::EntityCommands, prelude::*, ui::FocusPolicy};

use super::{
    dialog::SpawnDialog,
    interact_style::{Active, InteractStyle, InteractStyles},
    ui_actions::{Click, On},
    BUTTON_TEXT_STYLE,
};

pub trait UiBuilderExt: SpawnDialog {}

impl<'w, 's> UiBuilderExt for Commands<'w, 's> {}

pub trait UiChildBuilderExt<'w, 's>: SpawnButton<'w, 's> + SpawnSpacer<'w, 's> {}

impl<'w, 's, 'a> UiChildBuilderExt<'w, 's> for ChildBuilder<'w, 's, 'a> {}

#[derive(Resource)]
pub struct Held(Option<Entity>);

pub trait SpawnButton<'w, 's> {
    fn spawn_button<M>(
        &mut self,
        label: impl Into<String>,
        action: impl IntoSystem<(), (), M>,
    ) -> EntityCommands<'w, 's, '_>;
}

impl<'w, 's, 'a> SpawnButton<'w, 's> for ChildBuilder<'w, 's, 'a> {
    fn spawn_button<M>(
        &mut self,
        label: impl Into<String>,
        action: impl IntoSystem<(), (), M>,
    ) -> EntityCommands<'w, 's, '_> {
        let mut b = self.spawn((
            NodeBundle {
                style: Style {
                    border: UiRect::all(Val::Px(10.0)),
                    margin: UiRect::all(Val::Px(10.0)),
                    ..Default::default()
                },
                background_color: Color::WHITE.into(),
                focus_policy: FocusPolicy::Block,
                ..Default::default()
            },
            Interaction::default(),
            InteractStyles {
                active: Some(InteractStyle {
                    background: Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
                    ..Default::default()
                }),
                hover: Some(InteractStyle {
                    background: Some(Color::rgba(0.7, 0.7, 0.7, 1.0)),
                    ..Default::default()
                }),
                inactive: Some(InteractStyle {
                    background: Some(Color::rgba(0.4, 0.4, 0.4, 1.0)),
                    ..Default::default()
                }),
                disabled: Some(InteractStyle {
                    background: Some(Color::rgba(0.2, 0.2, 0.2, 1.0)),
                    ..Default::default()
                }),
            },
            Active(false),
            On::<Click>::new(action),
        ));

        b.with_children(|commands| {
            commands.spawn(
                TextBundle::from_section(label, BUTTON_TEXT_STYLE.get().unwrap().clone())
                    .with_text_alignment(TextAlignment::Center),
            );
        });

        b
    }
}

pub trait SpawnSpacer<'w, 's> {
    fn spacer(&mut self) -> EntityCommands<'w, 's, '_>;
}

impl<'w, 's, 'a> SpawnSpacer<'w, 's> for ChildBuilder<'w, 's, 'a> {
    fn spacer(&mut self) -> EntityCommands<'w, 's, '_> {
        self.spawn(NodeBundle {
            style: Style {
                flex_grow: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}
