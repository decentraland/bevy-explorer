use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    rpc::RpcResultSender,
    structs::{
        ActiveDialog, AppConfig, PermissionTarget, PermissionValue, PrimaryPlayerRes, SettingsTab,
        ShowSettingsEvent,
    },
};
use ipfs::CurrentRealm;
use scene_runner::{
    permissions::{PermissionLevel, PermissionManager, PermissionRequest, PermissionStrings},
    renderer_context::RendererSceneContext,
    ContainingScene,
};
use tokio::sync::oneshot::{channel, error::TryRecvError, Receiver};
use ui_core::{
    button::DuiButton,
    combo_box::ComboBox,
    ui_actions::{DataChanged, EventCloneExt, On, UiCaller},
};

use crate::login::config_file;

pub struct PermissionPlugin;

impl Plugin for PermissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PermissionManager>()
            .add_systems(PostUpdate, update_permissions);
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

    let Some(permit) = active_dialog.try_acquire() else {
        return;
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
                    debug!("no perm");
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
        commands.entity(popup.root).insert((
            permit,
            PermissionDialog {
                level: None,
                scene: req.scene,
                hash: hash.to_owned(),
                realm: req.realm.clone(),
            },
        ));
        pending.push((cancel_rx, popup.root, Some(req)));
        break;
    }
}

#[derive(Component)]
pub struct PermissionDialog {
    level: Option<PermissionLevel>,
    scene: Entity,
    hash: String,
    realm: String,
}
