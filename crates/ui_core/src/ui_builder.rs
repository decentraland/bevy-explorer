use bevy::{ecs::system::EntityCommands, prelude::*};
use common::util::TryChildBuilder;

pub trait SpawnSpacer {
    fn spacer(&mut self) -> EntityCommands<'_>;
}

impl SpawnSpacer for ChildBuilder<'_> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(NodeBundle {
            style: Node {
                flex_grow: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

impl SpawnSpacer for TryChildBuilder<'_> {
    fn spacer(&mut self) -> EntityCommands<'_> {
        self.spawn(NodeBundle {
            style: Node {
                flex_grow: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}
