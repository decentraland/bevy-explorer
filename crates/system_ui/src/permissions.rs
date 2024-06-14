use std::{collections::VecDeque, sync::Arc};

use bevy::{ecs::system::SystemParam, prelude::*, utils::HashMap};
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    rpc::RpcResultSender,
    structs::{AppConfig, PermissionType, PermissionValue, PrimaryPlayerRes},
};
use ipfs::CurrentRealm;
use scene_runner::{renderer_context::RendererSceneContext, ContainingScene};
use tokio::sync::{
    oneshot::{channel, error::TryRecvError, Receiver},
    OwnedSemaphorePermit, Semaphore,
};
use ui_core::{
    button::DuiButton,
    combo_box::ComboBox,
    ui_actions::{DataChanged, On, UiCaller},
};

use crate::login::config_file;

#[derive(Resource)]
pub struct ActiveDialog(Arc<Semaphore>);

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
    Scene(String),
    Realm(String),
    Global,
}

struct PermissionRequest {
    realm: String,
    scene: String,
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
        scene: String,
        additional: Option<String>,
    ) -> Receiver<bool> {
        let (sender, receiver) = channel();
        self.pending.push_back(PermissionRequest {
            realm,
            scene,
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
}

impl<'w, 's, T: Send + Sync + 'static> Permission<'w, 's, T> {
    pub fn check(
        &mut self,
        ty: PermissionType,
        scene: Entity,
        value: T,
        additional: Option<String>,
    ) {
        if !self.containing_scenes.get(self.player.0).contains(&scene) {
            return;
        }
        let Ok(hash) = self.scenes.get(scene).map(|ctx| &ctx.hash) else {
            return;
        };
        match self.config.get_permission(ty, &self.realm.address, hash) {
            common::structs::PermissionValue::Allow => self.success.push(value),
            common::structs::PermissionValue::Deny => self.fail.push(value),
            common::structs::PermissionValue::Ask | common::structs::PermissionValue::Default => {
                self.pending.push((
                    value,
                    self.manager
                        .request(ty, self.realm.address.clone(), hash.clone(), additional),
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
        self.success.drain(..)
    }

    pub fn drain_fail(&mut self) -> impl Iterator<Item = T> + '_ {
        self.fail.drain(..)
    }
}

#[allow(clippy::too_many_arguments)]
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
) {
    if manager.pending.is_empty() {
        return;
    }

    if config.is_changed() {
        manager.pending.retain(
            |req| match config.get_permission(req.ty, &req.realm, &req.scene) {
                PermissionValue::Allow => {
                    req.sender.clone().send(true);
                    false
                }
                PermissionValue::Deny => {
                    req.sender.clone().send(false);
                    false
                }
                _ => true,
            },
        );
    }

    let permit = match active_dialog.0.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return,
    };

    let active_scenes = containing_scene
        .get(player.0)
        .into_iter()
        .flat_map(|e| scenes.get(e).ok())
        .map(|ctx| (ctx.hash.as_str(), ctx.title.as_str()))
        .collect::<HashMap<_, _>>();

    while let Some(req) = manager.pending.pop_front() {
        if req.realm != current_realm.address {
            continue;
        }

        let Some(name) = active_scenes.get(req.scene.as_str()) else {
            continue;
        };

        let (title, body) = req.ty.data();
        let title = format!("Permission Request - {} - {}", name, title);
        let body = match req.additional {
            Some(add) => format!("{body}\n{add}"),
            None => body,
        };

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
                    PermissionLevel::Scene(scene) => config
                        .scene_permissions
                        .entry(scene.clone())
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
                            || {
                                println!("todo");
                            },
                        )],
                    )
                    .with_prop(
                        "options",
                        [
                            "Once",
                            "Always for Scene",
                            "Always for Realm",
                            "Always for All",
                        ]
                        .into_iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                    )
                    .with_prop(
                        "option-changed",
                        On::<DataChanged>::new(
                            |mut dialog: Query<&mut PermissionDialog>,
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
                                    1 => Some(PermissionLevel::Scene(dialog.scene.clone())),
                                    2 => Some(PermissionLevel::Realm(dialog.realm.clone())),
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
            realm: req.realm,
        });
        break;
    }
}

#[derive(Component)]
pub struct PermissionDialog {
    pub permit: OwnedSemaphorePermit,
    level: Option<PermissionLevel>,
    scene: String,
    realm: String,
}

trait PopupDisplay {
    fn data(&self) -> (String, String);
}

impl PopupDisplay for PermissionType {
    fn data(&self) -> (String, String) {
        let (t, b) = match self {
            PermissionType::MovePlayer => ("Move Avatar", "The scene wants permission to move your avatar within the scene bounds"),
            PermissionType::ForceCamera => ("Force Camera", "The scene wants permission to temporarily change the camera view"),
            PermissionType::PlayEmote => ("Play Emote", "The scene wants permission to make your avatar perform an emote"),
            PermissionType::SetLocomotion => ("Set Locomotion", "The scene wants permission to temporarily modify your avatar's locomotion settings"),
            PermissionType::HideAvatars => ("Hide Avatars", "The scene wants permission to temporarily hide player avatars"),
            PermissionType::DisableVoice => ("Disable Voice", "The scene wants permission to temporarily disable voice chat"),
            PermissionType::Teleport => ("Teleport", "The scene wants permission to teleport you to a new location"),
            PermissionType::ChangeRealm => ("Change Realm", "The scene wants permission to move you to a new realm"),
            PermissionType::SpawnPortable => ("Spawn Portable Experience", "The scene wants permission to spawn a portable experience"),
            PermissionType::KillPortables => ("Manage Portable Experiences", "The scene wants permission to manage your active portable experiences"),
            PermissionType::Web3 => ("Web3 Transaction", "The scene wants permission to initiate a web3 transaction with your wallet"),
            PermissionType::Fetch => ("Fetch Data", "The scene wants permission to fetch data from a remote server"),
            PermissionType::Websocket => ("Open Websocket", "The scene wants permission to open a socket to a remote server"),
            PermissionType::OpenUrl => ("Open Url", "The scene wants permission to open a url in your browser"),
        };

        (t.to_owned(), b.to_owned())
    }
}
