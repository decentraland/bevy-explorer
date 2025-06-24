use bevy::{ecs::system::EntityCommands, prelude::*};
use common::util::TryChildBuilder;

pub trait SpawnSpacer {
    fn spacer(&mut self) -> EntityCommands<'_>;
}

impl SpawnSpacer for ChildSpawnerCommands<'_> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(Node {
            flex_grow: 1.0,
            ..Default::default()
        })
    }
}

impl SpawnSpacer for TryChildBuilder<'_> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(Node {
            flex_grow: 1.0,
            ..Default::default()
        })
    }
}
