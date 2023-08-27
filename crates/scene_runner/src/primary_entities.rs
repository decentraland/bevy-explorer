use bevy::{ecs::system::SystemParam, prelude::*};
use common::structs::{PrimaryCamera, PrimaryUser};

#[derive(SystemParam)]
pub struct PrimaryEntities<'w, 's> {
    player: Query<'w, 's, Entity, With<PrimaryUser>>,
    camera: Query<'w, 's, Entity, With<PrimaryCamera>>,
}

impl<'w, 's> PrimaryEntities<'w, 's> {
    pub fn player(&self) -> Entity {
        self.player.single()
    }

    pub fn camera(&self) -> Entity {
        self.camera.single()
    }
}
