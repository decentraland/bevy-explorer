use std::collections::VecDeque;

use crate::{renderer_context::RendererSceneContext, ContainingScene};
use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    rpc::{RpcResultReceiver, RpcResultSender},
    structs::{AppConfig, PermissionType, PermissionUsed, PrimaryPlayerRes, SystemScene},
};
use ipfs::CurrentRealm;
use tokio::sync::oneshot::error::TryRecvError;

pub struct PermissionRequest {
    pub realm: String,
    pub scene: Entity,
    pub is_portable: bool,
    pub additional: Option<String>,
    pub ty: PermissionType,
    pub sender: RpcResultSender<bool>,
}

#[derive(Resource, Default)]
pub struct PermissionManager {
    pub pending: VecDeque<PermissionRequest>,
}

impl PermissionManager {
    fn request(
        &mut self,
        ty: PermissionType,
        realm: String,
        scene: Entity,
        is_portable: bool,
        additional: Option<String>,
    ) -> RpcResultReceiver<bool> {
        let (sender, receiver) = RpcResultSender::channel();
        self.pending.push_back(PermissionRequest {
            realm,
            scene,
            is_portable,
            ty,
            sender,
            additional,
        });
        receiver
    }
}

#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub struct Permission<'w, 's, T: Send + Sync + 'static> {
    pub success: Local<'s, Vec<(T, PermissionType, Option<String>, Entity)>>,
    pub fail: Local<'s, Vec<(T, PermissionType, Entity)>>,
    pub pending: Local<'s, Vec<(T, PermissionType, Entity, Option<String>, RpcResultReceiver<bool>)>>,
    config: Res<'w, AppConfig>,
    realm: Res<'w, CurrentRealm>,
    containing_scenes: ContainingScene<'w, 's>,
    player: Res<'w, PrimaryPlayerRes>,
    scenes: Query<'w, 's, &'static RendererSceneContext>,
    manager: ResMut<'w, PermissionManager>,
    system_scene: Option<Res<'w, SystemScene>>,
    uses: EventWriter<'w, PermissionUsed>,
}

impl<T: Send + Sync + 'static> Permission<'_, '_, T> {
    // in scene, hash, title, is_portable
    fn get_scene_info(&self, scene: Entity) -> Option<(bool, &str, &str, bool)> {
        let in_scene = self
            .containing_scenes
            .get_area(self.player.0, PLAYER_COLLIDER_RADIUS)
            .contains(&scene);
        self.scenes
            .get(scene)
            .map(|ctx| {
                (
                    in_scene,
                    ctx.hash.as_str(),
                    ctx.title.as_str(),
                    ctx.is_portable,
                )
            })
            .ok()
    }

    fn is_system_scene(&self, hash: &str) -> bool {
        self.system_scene
            .as_ref()
            .and_then(|ss| ss.hash.as_ref())
            .is_some_and(|sh| sh == hash)
    }

    pub fn check(
        &mut self,
        ty: PermissionType,
        scene: Entity,
        value: T,
        additional: Option<String>,
        allow_out_of_scene: bool,
    ) {
        let Some((in_scene, hash, _, is_portable)) = self.get_scene_info(scene) else {
            self.fail.push((value, ty, scene));
            return;
        };

        // allow system scene to do anything
        if self.is_system_scene(hash) {
            self.success.push((value, ty, additional, scene));
            return;
        };

        let perm = if !allow_out_of_scene && !in_scene {
            common::structs::PermissionValue::Ask
        } else {
            self.config
                .get_permission(ty, &self.realm.about_url, hash, is_portable)
        };

        debug!(
            "req {:?} for {:?} -> {:?} (in_scene = {}, allow_out = {}, real = {:?})",
            ty,
            scene,
            perm,
            in_scene,
            allow_out_of_scene,
            self.config
                .get_permission(ty, &self.realm.about_url, hash, is_portable)
        );
        match perm {
            common::structs::PermissionValue::Allow => {
                self.success.push((value, ty, additional, scene))
            }
            common::structs::PermissionValue::Deny => self.fail.push((value, ty, scene)),
            common::structs::PermissionValue::Ask => {
                self.pending.push((
                    value,
                    ty,
                    scene,
                    additional.clone(),
                    self.manager.request(
                        ty,
                        self.realm.about_url.clone(),
                        scene,
                        is_portable,
                        additional,
                    ),
                ));
            }
        }
    }

    pub fn check_unique(
        &mut self,
        ty: PermissionType,
        scene: Entity,
        value: T,
        additional: Option<String>,
        allow_out_of_scene: bool,
    ) where
        T: Eq,
    {
        if !self.iter_pending().any(|v| *v == value) {
            self.check(ty, scene, value, additional, allow_out_of_scene);
        }
    }

    fn update_pending(&mut self) {
        let pending = std::mem::take(&mut *self.pending);
        *self.pending = pending
            .into_iter()
            .flat_map(
                |(value, ty, scene, additional, mut rx)| match rx.try_recv() {
                    Ok(true) => {
                        self.success.push((value, ty, additional, scene));
                        None
                    }
                    Ok(false) => {
                        self.fail.push((value, ty, scene));
                        None
                    }
                    Err(TryRecvError::Closed) => {
                        let (_, _, _, is_portable) = self.get_scene_info(scene)?;
                        warn!("unexpected close of channel, re-requesting");
                        Some((
                            value,
                            ty,
                            scene,
                            additional.clone(),
                            self.manager.request(
                                ty,
                                self.realm.about_url.clone(),
                                scene,
                                is_portable,
                                additional,
                            ),
                        ))
                    }
                    Err(TryRecvError::Empty) => Some((value, ty, scene, additional, rx)),
                },
            )
            .collect();
    }

    pub fn drain_success(&mut self, ty: PermissionType) -> impl Iterator<Item = T> {
        self.update_pending();

        let (matching, not_matching): (Vec<_>, Vec<_>) = self
            .success
            .drain(..)
            .partition(|(_, perm_ty, _, _)| *perm_ty == ty);
        *self.success = not_matching;

        for item in matching.iter() {
            let (_, _, additional, scene) = item;
            let scene = *scene;
            if let Some((_, hash, _, _)) = self.get_scene_info(scene) {
                if !self.is_system_scene(hash) {
                    let used = PermissionUsed {
                        ty,
                        additional: additional.clone(),
                        scene: hash.to_owned(),
                        was_allowed: true,
                    };
                    self.uses.write(used);
                }
            }
        }
        matching.into_iter().map(|(value, _, _, _)| value)
    }

    pub fn drain_fail(&mut self, ty: PermissionType) -> impl Iterator<Item = T> + '_ {
        let (matching, not_matching): (Vec<_>, Vec<_>) = self
            .fail
            .drain(..)
            .partition(|(_, perm_ty, _)| *perm_ty == ty);
        *self.fail = not_matching;

        for item in matching.iter() {
            let (_, _, scene) = item;
            let scene = *scene;
            if let Some((_, hash, _, _)) = self.get_scene_info(scene) {
                if !self.is_system_scene(hash) {
                    let used = PermissionUsed {
                        ty,
                        additional: None,
                        scene: hash.to_owned(),
                        was_allowed: false,
                    };
                    self.uses.write(used);
                }
            }
        }
        matching.into_iter().map(|(value, _, _)| value)
    }

    pub fn iter_pending(&self) -> impl Iterator<Item = &T> + '_ {
        self.pending.iter().map(|(value, ..)| value)
    }
}
