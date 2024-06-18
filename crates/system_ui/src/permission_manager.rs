use std::{collections::VecDeque, sync::Arc};

use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    rpc::RpcResultSender,
    structs::{AppConfig, PermissionType, PermissionValue, PrimaryPlayerRes},
};
use ipfs::CurrentRealm;
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene, Toaster};
use tokio::sync::{
    oneshot::{channel, error::TryRecvError, Receiver},
    OwnedSemaphorePermit, Semaphore,
};
use ui_core::{
    button::DuiButton,
    combo_box::ComboBox,
    ui_actions::{Click, DataChanged, EventCloneExt, On, UiCaller},
};

use crate::{
    login::config_file,
    permissions::PermissionTarget,
    profile::{SettingsTab, ShowSettingsEvent},
};

#[derive(Resource)]
pub struct ActiveDialog(pub Arc<Semaphore>);

pub struct PermissionPlugin;

impl Plugin for PermissionPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ActiveDialog(Arc::new(Semaphore::new(1))))
            .init_resource::<PermissionManager>()
            .add_systems(PostUpdate, update_permissions);
    }
}

#[derive(Clone)]
pub enum PermissionLevel {
    Scene(Entity, String),
    Realm(String),
    Global,
}

struct PermissionRequest {
    realm: String,
    scene: Entity,
    is_portable: bool,
    additional: Option<String>,
    ty: PermissionType,
    sender: RpcResultSender<bool>,
}

#[derive(Resource, Default)]
struct PermissionManager {
    pending: VecDeque<PermissionRequest>,
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

#[derive(SystemParam)]
pub struct Permission<'w, 's, T: Send + Sync + 'static> {
    success: Local<'s, Vec<T>>,
    fail: Local<'s, Vec<T>>,
    pending: Local<'s, Vec<(T, Receiver<bool>)>>,
    config: Res<'w, AppConfig>,
    realm: Res<'w, CurrentRealm>,
    containing_scenes: ContainingScene<'w, 's>,
    player: Res<'w, PrimaryPlayerRes>,
    scenes: Query<'w, 's, &'static RendererSceneContext>,
    manager: ResMut<'w, PermissionManager>,
    toaster: Toaster<'w, 's>,
    ty: Local<'s, Option<PermissionType>>,
}

impl<'w, 's, T: Send + Sync + 'static> Permission<'w, 's, T> {
    pub fn check(
        &mut self,
        ty: PermissionType,
        scene: Entity,
        value: T,
        additional: Option<String>,
    ) {
        *self.ty = Some(ty);
        if !self.containing_scenes.get(self.player.0).contains(&scene) {
            return;
        }
        let Ok((hash, is_portable)) = self
            .scenes
            .get(scene)
            .map(|ctx| (&ctx.hash, ctx.is_portable))
        else {
            return;
        };
        match self
            .config
            .get_permission(ty, &self.realm.address, hash, is_portable)
        {
            common::structs::PermissionValue::Allow => self.success.push(value),
            common::structs::PermissionValue::Deny => self.fail.push(value),
            common::structs::PermissionValue::Ask => {
                self.pending.push((
                    value,
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

    pub fn drain_success(&mut self) -> impl Iterator<Item = T> + '_ {
        *self.pending = self
            .pending
            .drain(..)
            .flat_map(|(value, mut rx)| match rx.try_recv() {
                Ok(true) => {
                    self.success.push(value);
                    None
                }
                Ok(false) | Err(TryRecvError::Closed) => {
                    self.fail.push(value);
                    None
                }
                Err(TryRecvError::Empty) => Some((value, rx)),
            })
            .collect();

        if !self.success.is_empty() {
            let ty = self.ty.unwrap();
            self.toaster.add_clicky_toast(
                format!("{:?}", ty),
                ty.on_success(),
                ShowSettingsEvent(SettingsTab::Permissions).send_value_on::<Click>(),
            );
        }
        self.success.drain(..)
    }

    pub fn drain_fail(&mut self) -> impl Iterator<Item = T> + '_ {
        if !self.fail.is_empty() {
            let ty = self.ty.unwrap();
            self.toaster.add_clicky_toast(
                format!("{:?}", ty),
                ty.on_fail(),
                ShowSettingsEvent(SettingsTab::Permissions).send_value_on::<Click>(),
            );
        }
        self.fail.drain(..)
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_permissions(
    mut commands: Commands,
    mut manager: ResMut<PermissionManager>,
    active_dialog: Res<ActiveDialog>,
    current_realm: Res<CurrentRealm>,
    containing_scene: ContainingScene,
    player: Res<PrimaryPlayerRes>,
    scenes: Query<&RendererSceneContext>,
    dui: Res<DuiRegistry>,
    config: Res<AppConfig>,
    // scene cancel, dialog Entity, original request
    mut pending: Local<Vec<(Receiver<()>, Entity, Option<PermissionRequest>)>>,
) {
    let active_scenes = containing_scene.get(player.0);

    pending.retain_mut(|(cancel_rx, ent, req)| {
        // check if dialog has been cancelled ("manage permissions") or completed
        match cancel_rx.try_recv() {
            Ok(()) => {
                // cancelled, readd
                manager.pending.push_front(req.take().unwrap());
                return false;
            }
            Err(TryRecvError::Closed) => {
                // completed, drop
                return false;
            }
            Err(TryRecvError::Empty) => {
                // not completed or cancelled, retain
            }
        }

        // kill dialogs where the scene is no longer active
        if !active_scenes.contains(&req.as_ref().unwrap().scene) {
            commands.entity(*ent).despawn_recursive();
            req.take().unwrap().sender.send(false);
            return false;
        }

        true
    });

    if manager.pending.is_empty() {
        return;
    }

    if config.is_changed() {
        manager.pending.retain(|req| {
            let Ok(hash) = scenes.get(req.scene).map(|ctx| &ctx.hash) else {
                return false;
            };
            match config.get_permission(req.ty, &req.realm, hash, req.is_portable) {
                PermissionValue::Allow => {
                    req.sender.clone().send(true);
                    false
                }
                PermissionValue::Deny => {
                    req.sender.clone().send(false);
                    false
                }
                _ => true,
            }
        });
    }

    let permit = match active_dialog.0.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return,
    };

    while let Some(req) = manager.pending.pop_front() {
        if req.realm != current_realm.address {
            continue;
        }

        if !active_scenes.contains(&req.scene) {
            continue;
        }

        let Ok((hash, name)) = scenes.get(req.scene).map(|ctx| (&ctx.hash, &ctx.title)) else {
            continue;
        };

        let title = format!("Permission Request - {} - {}", name, req.ty.title());
        let body = match &req.additional {
            Some(add) => format!("{}\n{add}", req.ty.request()),
            None => req.ty.request(),
        };

        let (cancel_sx, cancel_rx) = channel();
        let cancel_sx = RpcResultSender::new(cancel_sx);

        let send = |value: PermissionValue| {
            let sender = req.sender.clone();
            let ty = req.ty;
            move |mut config: ResMut<AppConfig>, dialog: Query<&PermissionDialog>| {
                sender.send(matches!(value, PermissionValue::Allow));
                let Some(level) = dialog.get_single().ok().and_then(|p| p.level.as_ref()) else {
                    warn!("no perm");
                    return;
                };
                match level {
                    PermissionLevel::Scene(_, hash) => config
                        .scene_permissions
                        .entry(hash.clone())
                        .or_default()
                        .insert(ty, value),
                    PermissionLevel::Realm(realm) => config
                        .realm_permissions
                        .entry(realm.clone())
                        .or_default()
                        .insert(ty, value),
                    PermissionLevel::Global => config.default_permissions.insert(ty, value),
                };
                let config_file = config_file();
                if let Some(folder) = config_file.parent() {
                    std::fs::create_dir_all(folder).unwrap();
                }
                if let Err(e) =
                    std::fs::write(config_file, serde_json::to_string(&*config).unwrap())
                {
                    warn!("failed to write to config: {e}");
                }
            }
        };

        let is_portable = req.is_portable;
        let scene_ent = req.scene;
        let ty = req.ty;
        let popup = commands
            .spawn_template(
                &dui,
                "permission-text-dialog",
                DuiProps::default()
                    .with_prop("title", title)
                    .with_prop("body", body)
                    .with_prop(
                        "buttons",
                        vec![
                            DuiButton::new_enabled_and_close("Allow", send(PermissionValue::Allow)),
                            DuiButton::new_enabled_and_close("Deny", send(PermissionValue::Deny)),
                        ],
                    )
                    .with_prop(
                        "buttons2",
                        vec![DuiButton::new_enabled_and_close(
                            "Manage Permissions",
                            (move |mut target: ResMut<PermissionTarget>| {
                                target.scene = Some(scene_ent);
                                target.ty = Some(ty);
                            })
                            .pipe(ShowSettingsEvent(SettingsTab::Permissions).send_value())
                            .pipe(move || {
                                cancel_sx.clone().send(());
                            }),
                        )],
                    )
                    .with_prop(
                        "options",
                        if is_portable {
                            ["Once", "Always for Scene", "Always for All"]
                                .into_iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                        } else {
                            [
                                "Once",
                                "Always for Scene",
                                "Always for Realm",
                                "Always for All",
                            ]
                            .into_iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                        },
                    )
                    .with_prop(
                        "option-changed",
                        On::<DataChanged>::new(
                            move |mut dialog: Query<&mut PermissionDialog>,
                                  caller: Res<UiCaller>,
                                  combo: Query<&ComboBox>| {
                                let Ok(mut dialog) = dialog.get_single_mut() else {
                                    warn!("no dialog");
                                    return;
                                };

                                let Ok(combo) = combo.get(caller.0) else {
                                    warn!("no combo");
                                    return;
                                };

                                dialog.level = match combo.selected {
                                    0 => None,
                                    1 => Some(PermissionLevel::Scene(
                                        dialog.scene,
                                        dialog.hash.clone(),
                                    )),
                                    2 => {
                                        if is_portable {
                                            Some(PermissionLevel::Global)
                                        } else {
                                            Some(PermissionLevel::Realm(dialog.realm.clone()))
                                        }
                                    }
                                    3 => Some(PermissionLevel::Global),
                                    _ => unreachable!(),
                                };

                                warn!("ok");
                            },
                        ),
                    ),
            )
            .unwrap();
        commands.entity(popup.root).insert(PermissionDialog {
            permit,
            level: None,
            scene: req.scene,
            hash: hash.to_owned(),
            realm: req.realm.clone(),
        });
        pending.push((cancel_rx, popup.root, Some(req)));
        break;
    }
}

#[derive(Component)]
pub struct PermissionDialog {
    pub permit: OwnedSemaphorePermit,
    level: Option<PermissionLevel>,
    scene: Entity,
    hash: String,
    realm: String,
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
