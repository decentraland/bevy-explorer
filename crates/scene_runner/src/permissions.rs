use std::collections::VecDeque;

use crate::{renderer_context::RendererSceneContext, ContainingScene, Toaster};
use bevy::{ecs::system::SystemParam, prelude::*};
use common::{
    rpc::RpcResultSender,
    structs::{
        AppConfig, PermissionTarget, PermissionType, PrimaryPlayerRes, SettingsTab,
        ShowSettingsEvent,
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
    success: Local<'s, Vec<(T, PermissionType, Entity)>>,
    fail: Local<'s, Vec<(T, PermissionType, Entity)>>,
    pending: Local<'s, Vec<(T, PermissionType, Entity, Receiver<bool>)>>,
    config: Res<'w, AppConfig>,
    realm: Res<'w, CurrentRealm>,
    containing_scenes: ContainingScene<'w, 's>,
    player: Res<'w, PrimaryPlayerRes>,
    scenes: Query<'w, 's, &'static RendererSceneContext>,
    manager: ResMut<'w, PermissionManager>,
    pub toaster: Toaster<'w, 's>,
}

impl<'w, 's, T: Send + Sync + 'static> Permission<'w, 's, T> {
    fn get_hash(&self, scene: Entity) -> Option<(&str, bool)> {
        if !self.containing_scenes.get(self.player.0).contains(&scene) {
            return None;
        }
        self.scenes
            .get(scene)
            .map(|ctx| (ctx.hash.as_str(), ctx.is_portable))
            .ok()
    }

    pub fn check(
        &mut self,
        ty: PermissionType,
        scene: Entity,
        value: T,
        additional: Option<String>,
    ) {
        let Some((hash, is_portable)) = self.get_hash(scene) else {
            return;
        };
        match self
            .config
            .get_permission(ty, &self.realm.address, hash, is_portable)
        {
            common::structs::PermissionValue::Allow => self.success.push((value, ty, scene)),
            common::structs::PermissionValue::Deny => self.fail.push((value, ty, scene)),
            common::structs::PermissionValue::Ask => {
                self.pending.push((
                    value,
                    ty,
                    scene,
                    self.manager.request(
                        ty,
                        self.realm.address.clone(),
                        scene,
                        is_portable,
                        additional,
                    ),
                ));
            }
        }
    }

    fn update_pending(&mut self) {
        *self.pending = self
            .pending
            .drain(..)
            .flat_map(|(value, ty, scene, mut rx)| match rx.try_recv() {
                Ok(true) => {
                    self.success.push((value, ty, scene));
                    None
                }
                Ok(false) | Err(TryRecvError::Closed) => {
                    self.fail.push((value, ty, scene));
                    None
                }
                Err(TryRecvError::Empty) => Some((value, ty, scene, rx)),
            })
            .collect();
    }

    pub fn drain_success(&mut self) -> impl Iterator<Item = T> + '_ {
        self.update_pending();

        if let Some(last) = self.success.last() {
            let (_, ty, scene) = last;
            let (ty, scene) = (*ty, *scene);
            self.toaster.add_clicky_toast(
                format!("{:?}", ty),
                ty.on_success(),
                On::<Click>::new(
                    (move |mut target: ResMut<PermissionTarget>| {
                        target.scene = Some(scene);
                        target.ty = Some(ty);
                    })
                    .pipe(ShowSettingsEvent(SettingsTab::Permissions).send_value()),
                ),
            );
        }
        self.success.drain(..).map(|(value, _, _)| value)
    }

    pub fn drain_fail(&mut self) -> impl Iterator<Item = T> + '_ {
        if let Some(last) = self.fail.last() {
            let (_, ty, scene) = last;
            let (ty, scene) = (*ty, *scene);
            self.toaster.add_clicky_toast(
                format!("{:?}", ty),
                ty.on_fail(),
                On::<Click>::new(
                    (move |mut target: ResMut<PermissionTarget>| {
                        target.scene = Some(scene);
                        target.ty = Some(ty);
                    })
                    .pipe(ShowSettingsEvent(SettingsTab::Permissions).send_value()),
                ),
            );
        }
        self.fail.drain(..).map(|(value, _, _)| value)
    }
}

pub trait PermissionStrings {
    fn active(&self) -> &str;
    fn passive(&self) -> &str;
    fn title(&self) -> &str;
    fn request(&self) -> String;
    fn on_success(&self) -> String;

    fn on_fail(&self) -> String;
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

    fn on_success(&self) -> String {
        format!("The scene is {}", self.active())
    }
    fn on_fail(&self) -> String {
        format!("The scene was blocked from {}", self.active())
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
