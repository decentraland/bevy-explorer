use bevy::{ecs::system::EntityCommands, prelude::*};
use common::util::TryChildBuilder;

pub trait SpawnSpacer {
    fn spacer(&mut self) -> EntityCommands<'_>;
}

impl<'a> SpawnSpacer for ChildBuilder<'a> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(NodeBundle {
            style: Style {
                flex_grow: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

impl<'a> SpawnSpacer for TryChildBuilder<'a> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(NodeBundle {
            style: Style {
                flex_grow: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}
