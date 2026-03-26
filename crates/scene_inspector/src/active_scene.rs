use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use common::structs::PrimaryUser;
use dcl::interface::CrdtStore;
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene, SceneThreadHandle};

use crate::snapshot::{PendingSnapshotRequests, SnapshotCallback};

/// The scene currently targeted by inspector commands.
/// When None, commands fall back to the parcel scene the player is standing in.
#[derive(Resource, Default)]
pub struct ActiveInspectionScene(pub Option<Entity>);

#[derive(SystemParam)]
pub struct SceneResolver<'w, 's> {
    pub active: Res<'w, ActiveInspectionScene>,
    pub scenes: Query<'w, 's, (Entity, &'static mut RendererSceneContext)>,
    pub handles: Query<'w, 's, &'static SceneThreadHandle>,
    pub containing_scene: ContainingScene<'w, 's>,
    pub player: Query<'w, 's, Entity, With<PrimaryUser>>,
}

impl SceneResolver<'_, '_> {
    fn resolve_entity(&self) -> Result<Entity, String> {
        if let Some(ent) = self.active.0 {
            if self.scenes.contains(ent) {
                return Ok(ent);
            }
            return Err(
                "active inspection scene no longer exists; use /set_scene to update".into(),
            );
        }
        let player_ent = self
            .player
            .single()
            .map_err(|_| "no primary player".to_string())?;
        self.containing_scene
            .get_parcel(player_ent)
            .ok_or_else(|| "player is not in any scene".to_string())
    }

    pub fn resolve(&self) -> Result<(Entity, &RendererSceneContext), String> {
        let ent = self.resolve_entity()?;
        self.scenes
            .get(ent)
            .map_err(|_| "could not find scene context".to_string())
    }

    pub fn resolve_mut(&mut self) -> Result<(Entity, Mut<'_, RendererSceneContext>), String> {
        let ent = self.resolve_entity()?;
        self.scenes
            .get_mut(ent)
            .map_err(|_| "could not find scene context".to_string())
    }

    /// Send a `GetCrdtSnapshot` request to the active scene and register `callback` to be
    /// called when the snapshot arrives.  Returns `Err` if there is no active scene or the
    /// send fails.
    pub fn request_snapshot<F>(
        &self,
        pending: &mut PendingSnapshotRequests,
        callback: F,
    ) -> Result<(), String>
    where
        F: FnOnce(&CrdtStore) + Send + Sync + 'static,
    {
        let entity = self.resolve_entity()?;
        let handle = self
            .handles
            .get(entity)
            .map_err(|_| "scene has no thread handle".to_string())?;
        handle
            .sender
            .try_send(dcl::RendererResponse::GetCrdtSnapshot)
            .map_err(|_| "failed to send snapshot request to scene".to_string())?;
        pending.push(entity, Box::new(callback) as SnapshotCallback);
        Ok(())
    }
}
