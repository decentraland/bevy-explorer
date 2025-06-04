use std::collections::VecDeque;

use crate::{renderer_context::RendererSceneContext, ContainingScene, Toaster};
use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    rpc::RpcResultSender,
    structs::{
        AppConfig, PermissionTarget, PermissionType, PrimaryPlayerRes, SettingsTab,
        ShowSettingsEvent, SystemScene,
    },
};
use ipfs::CurrentRealm;
use tokio::sync::oneshot::{channel, error::TryRecvError, Receiver};
use ui_core::ui_actions::{Click, EventCloneExt, On};

#[derive(Clone)]
pub enum PermissionLevel {
    Scene(Entity, String),
    Realm(String),
    Global,
}

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
    ) -> Receiver<bool> {
        let (sender, receiver) = channel();
        self.pending.push_back(PermissionRequest {
            realm,
            scene,
            is_portable,
            ty,
            sender: RpcResultSender::new(sender),
            additional,
        });
        receiver
    }
}

#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub struct Permission<'w, 's, T: Send + Sync + 'static> {
    pub success: Local<'s, Vec<(T, PermissionType, Entity)>>,
    pub fail: Local<'s, Vec<(T, PermissionType, Entity)>>,
    pub pending: Local<'s, Vec<(T, PermissionType, Entity, Receiver<bool>)>>,
    config: Res<'w, AppConfig>,
    realm: Res<'w, CurrentRealm>,
    containing_scenes: ContainingScene<'w, 's>,
    player: Res<'w, PrimaryPlayerRes>,
    scenes: Query<'w, 's, &'static RendererSceneContext>,
    manager: ResMut<'w, PermissionManager>,
    pub toaster: Toaster<'w, 's>,
    system_scene: Option<Res<'w, SystemScene>>,
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
            self.success.push((value, ty, scene));
            return;
        };

        let perm = if !allow_out_of_scene && !in_scene {
            common::structs::PermissionValue::Ask
        } else {
            self.config
                .get_permission(ty, &self.realm.address, hash, is_portable)
        };

        debug!(
            "req {:?} for {:?} -> {:?} (in_scene = {}, allow_out = {}, real = {:?})",
            ty,
            scene,
            perm,
            in_scene,
            allow_out_of_scene,
            self.config
                .get_permission(ty, &self.realm.address, hash, is_portable)
        );
        match perm {
            common::structs::PermissionValue::Allow => self.success.push((value, ty, scene)),
            common::structs::PermissionValue::Deny => self.fail.push((value, ty, scene)),
            common::structs::PermissionValue::Ask => {
                self.pending.push((
                    value,
                    ty,
                    scene,
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
            .flat_map(|(value, ty, scene, mut rx)| match rx.try_recv() {
                Ok(true) => {
                    self.success.push((value, ty, scene));
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
                        self.manager.request(
                            ty,
                            self.realm.about_url.clone(),
                            scene,
                            is_portable,
                            None,
                        ),
                    ))
                }
                Err(TryRecvError::Empty) => Some((value, ty, scene, rx)),
            })
            .collect();
    }

    pub fn drain_success(&mut self, ty: PermissionType) -> impl Iterator<Item = T> {
        self.update_pending();

        let (matching, not_matching): (Vec<_>, Vec<_>) = self
            .success
            .drain(..)
            .partition(|(_, perm_ty, _)| *perm_ty == ty);
        *self.success = not_matching;

        if let Some(last) = matching.last() {
            let (_, _, scene) = last;
            let scene = *scene;
            if let Some((_, hash, title, is_portable)) = self.get_scene_info(scene) {
                if !self.is_system_scene(hash) {
                    let portable_name = is_portable.then_some(title);
                    self.toaster.add_clicky_toast(
                        format!("{ty:?}"),
                        ty.on_success(portable_name),
                        On::<Click>::new(
                            (move |mut target: ResMut<PermissionTarget>| {
                                target.scene = Some(scene);
                                target.ty = Some(ty);
                            })
                            .pipe(ShowSettingsEvent(SettingsTab::Permissions).send_value()),
                        ),
                    );
                }
            }
        }
        matching.into_iter().map(|(value, _, _)| value)
    }

    pub fn drain_fail(&mut self, ty: PermissionType) -> impl Iterator<Item = T> + '_ {
        let (matching, not_matching): (Vec<_>, Vec<_>) = self
            .fail
            .drain(..)
            .partition(|(_, perm_ty, _)| *perm_ty == ty);
        *self.fail = not_matching;

        if let Some(last) = matching.last() {
            let (_, _, scene) = last;
            let scene = *scene;
            if let Some((_, hash, title, is_portable)) = self.get_scene_info(scene) {
                if !self.is_system_scene(hash) {
                    let portable_name = is_portable.then_some(title);
                    self.toaster.add_clicky_toast(
                        format!("{ty:?}"),
                        ty.on_fail(portable_name),
                        On::<Click>::new(
                            (move |mut target: ResMut<PermissionTarget>| {
                                target.scene = Some(scene);
                                target.ty = Some(ty);
                            })
                            .pipe(ShowSettingsEvent(SettingsTab::Permissions).send_value()),
                        ),
                    );
                }
            }
        }
        matching.into_iter().map(|(value, _, _)| value)
    }

    pub fn iter_pending(&self) -> impl Iterator<Item = &T> + '_ {
        self.pending.iter().map(|(value, ..)| value)
    }
}

pub trait PermissionStrings {
    fn active(&self) -> &str;
    fn passive(&self) -> &str;
    fn title(&self) -> &str;
    fn request(&self) -> String;
    fn on_success(&self, portable: Option<&str>) -> String;

    fn on_fail(&self, portable: Option<&str>) -> String;
    fn description(&self) -> String;
}

impl PermissionStrings for PermissionType {
    fn title(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "Move Avatar",
            PermissionType::ForceCamera => "Force Camera",
            PermissionType::PlayEmote => "Play Emote",
            PermissionType::SetLocomotion => "Set Locomotion",
            PermissionType::HideAvatars => "Hide Avatars",
            PermissionType::DisableVoice => "Disable Voice",
            PermissionType::Teleport => "Teleport",
            PermissionType::ChangeRealm => "Change Realm",
            PermissionType::SpawnPortable => "Spawn Portable Experience",
            PermissionType::KillPortables => "Manage Portable Experiences",
            PermissionType::Web3 => "Web3 Transaction",
            PermissionType::Fetch => "Fetch Data",
            PermissionType::Websocket => "Open Websocket",
            PermissionType::OpenUrl => "Open Url",
        }
    }

    fn request(&self) -> String {
        format!("The scene wants permission to {}", self.passive())
    }

    fn description(&self) -> String {
        format!(
            "This permission is requested when scene attempts to {}",
            self.passive()
        )
    }

    fn on_success(&self, portable: Option<&str>) -> String {
        format!(
            "{} is {} (click to manage)",
            match portable {
                Some(portable) => format!("The portable scene {portable}"),
                None => "The scene".to_owned(),
            },
            self.active()
        )
    }
    fn on_fail(&self, portable: Option<&str>) -> String {
        format!(
            "{} was blocked from {} (click to manage)",
            match portable {
                Some(portable) => format!("The portable scene {portable}"),
                None => "The scene".to_owned(),
            },
            self.active()
        )
    }

    fn passive(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "move your avatar within the scene bounds",
            PermissionType::ForceCamera => "temporarily change the camera view",
            PermissionType::PlayEmote => "make your avatar perform an emote",
            PermissionType::SetLocomotion => "temporarily modify your avatar's locomotion settings",
            PermissionType::HideAvatars => "temporarily hide player avatars",
            PermissionType::DisableVoice => "temporarily disable voice chat",
            PermissionType::Teleport => "teleport you to a new location",
            PermissionType::ChangeRealm => "move you to a new realm",
            PermissionType::SpawnPortable => "spawn a portable experience",
            PermissionType::KillPortables => "manage your active portable experiences",
            PermissionType::Web3 => "initiate a web3 transaction with your wallet",
            PermissionType::Fetch => "fetch data from a remote server",
            PermissionType::Websocket => "open a web socket to communicate with a remote server",
            PermissionType::OpenUrl => "open a url in your browser",
        }
    }

    fn active(&self) -> &str {
        match self {
            PermissionType::MovePlayer => "moving your avatar",
            PermissionType::ForceCamera => "enforcing the camera view",
            PermissionType::PlayEmote => "making your avatar perform an emote",
            PermissionType::SetLocomotion => "enforcing your locomotion settings",
            PermissionType::HideAvatars => "hiding some avatars",
            PermissionType::DisableVoice => "disabling voice communications",
            PermissionType::Teleport => "teleporting you to a new location",
            PermissionType::ChangeRealm => "teleporting you to a new realm",
            PermissionType::SpawnPortable => "spawning a portable experience",
            PermissionType::KillPortables => "managing your active portables",
            PermissionType::Web3 => "initiating a web3 transaction",
            PermissionType::Fetch => "fetching remote data",
            PermissionType::Websocket => "opening a websocket",
            PermissionType::OpenUrl => "opening a url in your browser",
        }
    }
}
