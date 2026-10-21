pub mod change_realm;
pub mod chat;
pub mod crash_report;
pub mod login;
pub mod oow;
pub mod sysinfo;
pub mod version_check;

use std::collections::BTreeMap;

use bevy::prelude::*;

use change_realm::ChangeRealmPlugin;
use common::{
    inputs::SystemAction,
    rpc::RpcStreamSender,
    sets::SetupSets,
    structs::{ActiveDialog, AppConfig, CurrentRealm, PermissionUsed, ToolTips, UiRoot, ZOrder},
};
use input_manager::{InputManager, InputPriority, MouseInteractionComponent};
use login::LoginPlugin;
use scene_runner::{
    permissions::PermissionManager, renderer_context::RendererSceneContext, Toaster,
};
use system_bridge::{
    NativeUi, PermanentPermissionItem, PermissionLevel, PermissionRequestEvent, PermissionValue,
    SystemApi,
};

use crate::oow::OowUiPlugin;

use self::{chat::ChatPanelPlugin, sysinfo::SysInfoPanelPlugin};

pub struct SystemUiPlugin;

impl Plugin for SystemUiPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SystemUiRoot(Entity::PLACEHOLDER))
            .init_resource::<ActiveDialog>()
            .init_resource::<ToolTips>()
            .init_resource::<PermissionManager>()
            .add_event::<PermissionUsed>();
        app.add_systems(
            Startup,
            setup.in_set(SetupSets::Init).before(SetupSets::Main),
        );
        app.add_systems(Update, toggle_system_ui);

        app.add_plugins((
            SysInfoPanelPlugin,
            ChatPanelPlugin,
            LoginPlugin,
            ChangeRealmPlugin,
            OowUiPlugin,
        ));

        app.add_systems(PostUpdate, handle_scene_permissions);
    }
}

#[derive(Resource)]
struct SystemUiRoot(Entity);

#[allow(clippy::type_complexity)]
fn setup(mut commands: Commands, mut ui_root: ResMut<SystemUiRoot>, native_ui: Res<NativeUi>) {
    let show = native_ui.login;

    let root = commands
        .spawn((
            Node {
                display: if show { Display::Flex } else { Display::None },
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..Default::default()
            },
            ZOrder::SystemUi.default(),
            UiRoot,
        ))
        .id();
    ui_root.0 = root;

    // interaction component
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            top: Val::Px(0.0),
            bottom: Val::Px(0.0),
            ..Default::default()
        },
        ZOrder::MouseInteractionComponent.default(),
        Interaction::default(),
        MouseInteractionComponent,
    ));
}

fn toggle_system_ui(
    mut toast: Toaster,
    input_manager: InputManager,
    mut root: Query<&mut Node, With<UiRoot>>,
) {
    if input_manager.just_down(SystemAction::ShowDebugUi, InputPriority::None) {
        let Ok(mut root) = root.single_mut() else {
            warn!("no root");
            return;
        };

        if root.display == Display::Flex {
            toast.add_toast("hide ui", "System ui hidden (press PageUp to toggle)");
            root.display = Display::None;
        } else {
            root.display = Display::Flex;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_scene_permissions(
    mut manager: ResMut<PermissionManager>,
    mut config: ResMut<AppConfig>,
    mut system_events: EventReader<SystemApi>,
    mut requests_streams: Local<Vec<RpcStreamSender<PermissionRequestEvent>>>,
    mut used_streams: Local<Vec<RpcStreamSender<PermissionUsed>>>,
    // corresponds to the items in the manager deque (None = never streamed)
    mut permission_ids: Local<Vec<Option<usize>>>,
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
                    let (Some(handle), Ok(hash)) =
                        (handle, scenes.get(req.scene).map(|ctx| &ctx.hash))
                    else {
                        continue;
                    };

                    let _ = stream.send(PermissionRequestEvent::Request(
                        system_bridge::PermissionRequest {
                            ty: req.ty,
                            additional: req.additional.clone(),
                            scene: hash.to_string(),
                            id: *handle,
                        },
                    ));
                }

                requests_streams.push(stream.clone())
            }
            SystemApi::GetPermissionUsedStream(stream) => used_streams.push(stream.clone()),
            SystemApi::SetSinglePermission(result) => {
                let Some((index, _)) = permission_ids
                    .iter()
                    .enumerate()
                    .find(|(_, id)| **id == Some(result.id))
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
                platform::write_config_file(&*config);
            }
            SystemApi::GetPermanentPermissions(level, sender) => {
                let perms = match level {
                    PermissionLevel::Scene(hash) => config.scene_permissions.get(hash),
                    // stored under the current about_url (see SetPermanentPermission above)
                    PermissionLevel::Realm(realm) if current_realm.about_url.starts_with(realm) => {
                        config.realm_permissions.get(&current_realm.about_url)
                    }
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
            permission_ids.push(None);
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

        let next_id = needs_request.then(|| {
            let next_id = *inc;
            *inc += 1;

            for req_stream in requests_streams.iter() {
                let _ = req_stream.send(PermissionRequestEvent::Request(
                    system_bridge::PermissionRequest {
                        ty: req.ty,
                        additional: req.additional.clone(),
                        scene: hash.to_string(),
                        id: next_id,
                    },
                ));
            }

            next_id
        });

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

    // send out resolved items, and tell the streams any request they were shown is no longer open
    for (index, result) in resolved.iter().rev() {
        let id = permission_ids.remove(*index);
        let perm = manager.pending.remove(*index).unwrap();
        perm.sender.send(*result);

        if let Some(id) = id {
            for req_stream in requests_streams.iter() {
                let _ = req_stream.send(PermissionRequestEvent::Cancelled { id });
            }
        }
    }

    // send out uses
    for usage in uses.read() {
        for used_stream in used_streams.iter() {
            let _ = used_stream.send(usage.clone());
        }
    }
}
