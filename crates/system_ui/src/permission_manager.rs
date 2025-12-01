use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiProps, DuiRegistry};
use common::{
    dynamics::PLAYER_COLLIDER_RADIUS,
    rpc::{RpcResultReceiver, RpcResultSender, RpcStreamSender},
    structs::{
        ActiveDialog, AppConfig, PermissionLevel, PermissionStrings, PermissionTarget,
        PermissionUsed, PermissionValue, PrimaryPlayerRes, SettingsTab, ShowSettingsEvent, ZOrder,
    },
};
use ipfs::CurrentRealm;
use scene_runner::{
    initialize_scene::LiveScenes,
    permissions::{PermissionManager, PermissionRequest},
    renderer_context::RendererSceneContext,
    ContainingScene, Toaster,
};
use system_bridge::{NativeUi, PermanentPermissionItem, SystemApi};
use tokio::sync::oneshot::error::TryRecvError;
use ui_core::{
    button::DuiButton,
    combo_box::ComboBox,
    ui_actions::{Click, DataChanged, EventCloneExt, On, UiCaller},
};

pub struct PermissionPlugin;

impl Plugin for PermissionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PermissionManager>()
            .add_event::<PermissionUsed>();

        let native_ui = app.world().resource::<NativeUi>();
        if native_ui.permissions {
            app.add_systems(PostUpdate, (update_permissions, show_permission_toasts));
        } else {
            app.add_systems(PostUpdate, handle_scene_permissions);
        }
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
    mut displayed_dialogs: Local<Vec<(RpcResultReceiver<()>, Entity, Option<PermissionRequest>)>>,
) {
    let active_scenes = containing_scene.get_area(player.0, PLAYER_COLLIDER_RADIUS);

    displayed_dialogs.retain_mut(|(cancel_rx, ent, req)| {
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

        // kill/requeue dialogs where the scene is no longer active
        if !active_scenes.contains(&req.as_ref().unwrap().scene) {
            if let Ok(mut commands) = commands.get_entity(*ent) {
                commands.despawn();
            }
            let req = req.take().unwrap();
            if scenes.get(req.scene).is_ok() {
                manager.pending.push_front(req);
            }
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

    let mut repush_requests = Vec::default();
    while let Some(req) = manager.pending.pop_front() {
        if req.realm != current_realm.about_url && !req.is_portable {
            continue;
        }

        if !active_scenes.contains(&req.scene) {
            repush_requests.push(req);
            continue;
        }

        let Ok((hash, name)) = scenes.get(req.scene).map(|ctx| (&ctx.hash, &ctx.title)) else {
            continue;
        };

        // final check before dialog
        match config.get_permission(req.ty, &req.realm, hash, req.is_portable) {
            PermissionValue::Allow => {
                req.sender.clone().send(true);
                continue;
            }
            PermissionValue::Deny => {
                req.sender.clone().send(false);
                continue;
            }
            _ => (),
        }

        let title = format!("Permission Request - {} - {}", name, req.ty.title());
        let body = match &req.additional {
            Some(add) => format!("{}\n{add}", req.ty.request()),
            None => req.ty.request(),
        };

        let (cancel_sx, cancel_rx) = RpcResultSender::channel();

        let send = |value: PermissionValue| {
            let sender = req.sender.clone();
            let ty = req.ty;
            move |mut config: ResMut<AppConfig>, dialog: Query<&PermissionDialog>| {
                sender.send(matches!(value, PermissionValue::Allow));
                let Some(level) = dialog.single().ok().and_then(|p| p.level.as_ref()) else {
                    debug!("no perm");
                    return;
                };
                match level {
                    PermissionLevel::Scene(hash) => config
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
                platform::write_config_file(&*config);
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
                            DuiButton::new_enabled_and_close_happy(
                                "Allow",
                                send(PermissionValue::Allow),
                            ),
                            DuiButton::new_enabled_and_close_sad(
                                "Deny",
                                send(PermissionValue::Deny),
                            ),
                        ],
                    )
                    .with_prop(
                        "buttons2",
                        vec![DuiButton::new_enabled_and_close_silent(
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
                                let Ok(mut dialog) = dialog.single_mut() else {
                                    warn!("no dialog");
                                    return;
                                };

                                let Ok(combo) = combo.get(caller.0) else {
                                    warn!("no combo");
                                    return;
                                };

                                dialog.level = match combo.selected {
                                    0 => None,
                                    1 => Some(PermissionLevel::Scene(dialog.hash.clone())),
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
                hash: hash.to_owned(),
                realm: req.realm.clone(),
            },
            ZOrder::Permission.default(),
        ));
        displayed_dialogs.push((cancel_rx, popup.root, Some(req)));
        break;
    }

    for request in repush_requests.into_iter().rev() {
        manager.pending.push_front(request);
    }
}

#[derive(Component)]
pub struct PermissionDialog {
    level: Option<PermissionLevel>,
    hash: String,
    realm: String,
}

fn show_permission_toasts(
    mut toaster: Toaster,
    mut uses: EventReader<PermissionUsed>,
    live_scenes: Res<LiveScenes>,
    scenes: Query<&RendererSceneContext>,
) {
    for usage in uses.read() {
        let Some(scene) = live_scenes.scenes.get(&usage.scene).copied() else {
            error!("no scene for perm request");
            continue;
        };

        let Ok(ctx) = scenes.get(scene) else {
            continue;
        };

        let portable_name = ctx.is_portable.then_some(ctx.title.as_str());
        let ty = usage.ty;
        let message = if usage.was_allowed {
            usage
                .ty
                .on_success(portable_name, usage.additional.as_deref())
        } else {
            usage.ty.on_fail(portable_name)
        };

        toaster.add_clicky_toast(
            format!("{:?} {:?} {:?}", ty, usage.scene, usage.additional),
            message,
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

#[allow(clippy::too_many_arguments)]
pub fn handle_scene_permissions(
    mut manager: ResMut<PermissionManager>,
    mut config: ResMut<AppConfig>,
    mut system_events: EventReader<SystemApi>,
    mut requests_streams: Local<
        Vec<RpcStreamSender<system_bridge::PermissionRequest>>,
    >,
    mut used_streams: Local<Vec<RpcStreamSender<PermissionUsed>>>,
    // corresponds to the items in the manager deque
    mut permission_ids: Local<Vec<usize>>,
    mut inc: Local<usize>,
    scenes: Query<&RendererSceneContext>,
    current_realm: Res<CurrentRealm>,
    mut uses: EventReader<PermissionUsed>,
) {
    // gather any resolved permissions in order
    // (manager.pending index => result)
    let mut resolved = BTreeMap::default();

    for ev in system_events.read() {
        // handle events
        match ev {
            SystemApi::GetPermissionRequestStream(stream) => {
                // send any current outstanding requests when a new stream is attached
                for (handle, req) in permission_ids.iter().zip(manager.pending.iter()) {
                    let Ok(hash) = scenes.get(req.scene).map(|ctx| &ctx.hash) else {
                        continue;
                    };

                    let _ = stream.send(system_bridge::PermissionRequest {
                        ty: req.ty,
                        additional: req.additional.clone(),
                        scene: hash.clone(),
                        id: *handle,
                    });
                }

                requests_streams.push(stream.clone())
            }
            SystemApi::GetPermissionUsedStream(stream) => used_streams.push(stream.clone()),
            SystemApi::SetSinglePermission(result) => {
                let Some((index, _)) = permission_ids
                    .iter()
                    .enumerate()
                    .find(|(_, id)| **id == result.id)
                else {
                    warn!("permission request with id {} not found", result.id);
                    continue;
                };

                resolved.insert(index, result.allow);
            }
            SystemApi::SetPermanentPermission(result) => {
                let store = match &result.level {
                    PermissionLevel::Scene(hash) => {
                        config.scene_permissions.entry(hash.clone()).or_default()
                    }
                    PermissionLevel::Realm(realm) => {
                        if current_realm.about_url.starts_with(realm) {
                            config
                                .realm_permissions
                                .entry(current_realm.about_url.clone())
                                .or_default()
                        } else {
                            warn!("permission realm didn't match current realm: {} is not initial segment of {}", realm, current_realm.about_url);
                            continue;
                        }
                    }
                    PermissionLevel::Global => &mut config.default_permissions,
                };

                if let Some(value) = result.allow {
                    store.insert(result.ty, value);
                } else {
                    store.remove(&result.ty);
                }
            }
            SystemApi::GetPermanentPermissions(level, sender) => {
                let perms = match level {
                    PermissionLevel::Scene(hash) => config.scene_permissions.get(hash),
                    PermissionLevel::Realm(realm) => config.realm_permissions.get(realm),
                    PermissionLevel::Global => Some(&config.default_permissions),
                };

                sender.send(
                    perms
                        .map(|h| {
                            h.iter()
                                .map(|(p, v)| PermanentPermissionItem { ty: *p, allow: *v })
                                .collect()
                        })
                        .unwrap_or_default(),
                )
            }
            _ => (),
        }
    }

    if config.is_changed() {
        // check for anything that has become determinable
        for (i, req) in manager.pending.iter().enumerate() {
            let Ok(hash) = scenes.get(req.scene).map(|ctx| &ctx.hash) else {
                resolved.insert(i, false);
                continue;
            };

            match config.get_permission(req.ty, &req.realm, hash, req.is_portable) {
                PermissionValue::Allow => resolved.insert(i, true),
                PermissionValue::Deny => resolved.insert(i, false),
                PermissionValue::Ask => None,
            };
        }
    }

    // send any new requests
    for i in permission_ids.len()..manager.pending.len() {
        let req = manager.pending.get(i).unwrap();

        let Ok(hash) = scenes.get(req.scene).map(|ctx| &ctx.hash) else {
            resolved.insert(i, false);
            continue;
        };

        // make sure it needs to be sent
        let needs_request = match config.get_permission(req.ty, &req.realm, hash, req.is_portable) {
            PermissionValue::Allow => {
                resolved.insert(i, true);
                false
            }
            PermissionValue::Deny => {
                resolved.insert(i, false);
                false
            }
            PermissionValue::Ask => true,
        };

        let next_id = *inc;
        if needs_request {
            *inc += 1;

            for req_stream in requests_streams.iter() {
                let _ = req_stream.send(system_bridge::PermissionRequest {
                    ty: req.ty,
                    additional: req.additional.clone(),
                    scene: hash.clone(),
                    id: next_id,
                });
            }
        }

        permission_ids.push(next_id);
    }

    // clean up any failed requests
    for (i, req) in manager.pending.iter().enumerate() {
        if req.realm != current_realm.about_url && !req.is_portable {
            resolved.insert(i, false);
        }
    }

    // clean up streams
    requests_streams.retain(|s| !s.is_closed());
    used_streams.retain(|s| !s.is_closed());

    // send out resolved items
    for (index, result) in resolved.iter().rev() {
        permission_ids.remove(*index);
        let perm = manager.pending.remove(*index).unwrap();
        perm.sender.send(*result);
    }

    // send out uses
    for usage in uses.read() {
        for used_stream in used_streams.iter() {
            let _ = used_stream.send(usage.clone());
        }
    }
}
