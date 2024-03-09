use bevy::{ecs::system::EntityCommands, prelude::*};

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
