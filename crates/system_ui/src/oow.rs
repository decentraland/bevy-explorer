use std::f32::consts::TAU;

use alloy_core::primitives::Address;
use analytics::{
    data_definition::{SegmentEvent, SegmentEventLoading},
    segment_system::SegmentMetricsEvents,
};
use bevy::{image::ImageLoaderSettings, prelude::*};
use bevy_dui::{DuiEntities, DuiEntityCommandsExt, DuiProps};
use common::{
    rpc::RpcStreamSender,
    structs::{CurrentRealm, PrimaryUser, WorldHold, ZOrder},
    util::TryPushChildrenEx,
};
use scene_runner::{
    initialize_scene::SceneLoading,
    renderer_context::{
        RendererSceneContext, SCENE_NOT_RESPONDING_DISPLAY_AFTER, SCENE_NOT_RESPONDING_TIMEOUT,
    },
    update_world::gltf_container::GltfLoadingCount,
    ContainingScene, OutOfWorld,
};
use system_bridge::{NativeUi, SceneLoadingUi, SystemApi};
use ui_core::ui_actions::{Click, EventDefaultExt};
use wallet::Wallet;

use crate::change_realm::ChangeRealmDialog;

/// Extracts scene loading info: (title, pending_assets_count)
fn get_scene_loading_info(
    player: Entity,
    now: f64,
    containing_scene: &ContainingScene,
    scenes: &Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
) -> (String, Option<u32>) {
    let scene = containing_scene
        .get_parcel_oow(player)
        .and_then(|scene| scenes.get(scene).ok());

    let mut title = scene
        .map(|(context, _)| context.title.clone())
        .unwrap_or_else(|| "Scene".to_owned());

    let pending_assets = scene.and_then(|(_, gltf_count)| gltf_count.map(|c| c.0 as u32));

    // Assets are loaded (nothing pending) but a tick has been in-flight for a while: the scene
    // script is stalling. Surface a countdown to the not-responding timeout, after which
    // handle_out_of_world lets the player into the world.
    if let Some((context, _)) = scene {
        let in_flight_for = (now - context.last_sent) as f32;
        if pending_assets.unwrap_or(0) == 0
            && context.in_flight()
            && in_flight_for >= SCENE_NOT_RESPONDING_DISPLAY_AFTER.as_secs_f32()
        {
            let remaining = (SCENE_NOT_RESPONDING_TIMEOUT.as_secs_f32() - in_flight_for).max(0.0);
            title = format!(
                "{title} (not responding... timeout in {} seconds)",
                remaining.ceil() as u32
            );
        }
    }

    (title, pending_assets)
}

pub struct OowUiPlugin;

impl Plugin for OowUiPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<NativeUi>().loading_scene {
            app.add_systems(
                Update,
                update_loading_scene_dialog.run_if(not(resource_exists::<WorldHold>)),
            );
        } else {
            app.add_systems(
                Update,
                update_loading_backdrop.run_if(not(resource_exists::<WorldHold>)),
            );
        }
        app.add_systems(
            Update,
            (
                pipe_scene_loading_ui_stream,
                animate_logo_pulse,
                track_loading_episodes,
            ),
        );
    }
}

/// Whether the loading screen covers the world: a connected player out of world, not held
/// for a lobby.
fn loading_visible(wallet: &Wallet, oow_player: bool, world_hold: bool) -> bool {
    wallet.address().is_some() && oow_player && !world_hold
}

struct LoadingEpisode {
    id: u64,
    started_at: f64,
    realm_bucket: String,
    // the scene at the player's parcel failed to load, as last seen while loading
    scene_failed: bool,
}

/// "world" on a worlds server, "genesis" on a catalyst of the base domain, else "other".
fn realm_bucket(realm: &CurrentRealm) -> String {
    if realm.about_url.is_empty() {
        return "unknown".to_owned();
    }
    if ipfs::world_access::world_name(&realm.about_url).is_some() {
        return "world".to_owned();
    }
    let host = realm
        .about_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default();
    let base = common::base_domain::get();
    if !realm.is_local() && (host == base || host.ends_with(&format!(".{base}"))) {
        "genesis".to_owned()
    } else {
        "other".to_owned()
    }
}

fn parcel_scene_failed(
    player: Entity,
    containing_scene: &ContainingScene,
    loading: &Query<&SceneLoading>,
) -> bool {
    containing_scene
        .get_parcel_oow(player)
        .and_then(|scene| loading.get(scene).ok())
        .is_some_and(|state| matches!(state, SceneLoading::Failed))
}

/// "Loading Event" started/completed for every user-visible load (the same predicate the
/// loading screen shows on). Background streaming never opens an episode.
#[allow(clippy::too_many_arguments)]
fn track_loading_episodes(
    time: Res<Time>,
    wallet: Res<Wallet>,
    world_hold: Option<Res<WorldHold>>,
    current_realm: Res<CurrentRealm>,
    player: Query<(Entity, Has<OutOfWorld>), With<PrimaryUser>>,
    containing_scene: ContainingScene,
    loading: Query<&SceneLoading>,
    mut metrics: ResMut<SegmentMetricsEvents>,
    mut episode: Local<Option<LoadingEpisode>>,
    mut next_id: Local<u64>,
    mut login: Local<Option<Address>>,
    mut loaded_since_login: Local<bool>,
    // the realm the last episode landed in: every realm change goes through an episode
    mut landed_realm: Local<String>,
) {
    let (player, oow) = player.single().unwrap_or((Entity::PLACEHOLDER, false));
    let visible = loading_visible(&wallet, oow, world_hold.is_some());
    let now = time.elapsed_secs_f64();

    let address = wallet.address();
    if address != *login {
        *login = address;
        *loaded_since_login = false;
    }

    match (episode.as_mut(), visible) {
        (None, true) => {
            let when = if !*loaded_since_login {
                "on_explorer_ready"
            } else if *landed_realm != current_realm.about_url {
                "on_changerealm"
            } else {
                "on_teleport"
            };
            *loaded_since_login = true;
            *next_id += 1;
            let bucket = realm_bucket(&current_realm);
            let mut event = SegmentEventLoading::base("started", *next_id, bucket.clone());
            event.when = Some(when.to_owned());
            event.is_world = Some(bucket == "world");
            metrics.add_event(SegmentEvent::Loading(Box::new(event)));
            *episode = Some(LoadingEpisode {
                id: *next_id,
                started_at: now,
                realm_bucket: bucket,
                scene_failed: false,
            });
        }
        (Some(current), true) => {
            current.scene_failed |= parcel_scene_failed(player, &containing_scene, &loading);
        }
        (Some(current), false) => {
            // a failed load keeps its entity, so it still shows once the player is let in
            let failed =
                current.scene_failed || parcel_scene_failed(player, &containing_scene, &loading);
            let mut event =
                SegmentEventLoading::base("completed", current.id, current.realm_bucket.clone());
            event.is_world = Some(current.realm_bucket == "world");
            event.duration_ms = Some(((now - current.started_at) * 1000.0).round() as i64);
            let dismissed_by = if login.is_none() {
                // logged out mid-load
                "cancelled"
            } else if failed {
                "error"
            } else {
                "completion"
            };
            event.dismissed_by = Some(dismissed_by.to_owned());
            metrics.add_event(SegmentEvent::Loading(Box::new(event)));
            *episode = None;
            landed_realm.clone_from(&current_realm.about_url);
        }
        (None, false) => (),
    }
}

#[allow(clippy::too_many_arguments)]
fn update_loading_scene_dialog(
    mut commands: Commands,
    time: Res<Time>,
    wallet: Res<Wallet>,
    oow: Query<&OutOfWorld>,
    mut last_count: Local<usize>,
    mut dialog: Local<Option<Entity>>,
    dui: Res<bevy_dui::DuiRegistry>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    scenes: Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
    template: Query<&DuiEntities>,
    mut text: Query<&mut Text>,
) {
    if wallet.address().is_none() || oow.is_empty() {
        if let Some(ent) = dialog.take() {
            commands.entity(ent).despawn();
        }
        *last_count = 0;
        return;
    }

    let Ok(player) = player.single() else {
        return;
    };

    let (title_text, pending_assets) =
        get_scene_loading_info(player, time.elapsed_secs_f64(), &containing_scene, &scenes);
    let state_text = pending_assets
        .map(|count| format!("{} assets", count))
        .unwrap_or_else(|| "assets".to_owned());

    match dialog.as_ref() {
        Some(ent) => {
            let Ok(components) = template.get(*ent) else {
                warn!("no components?!");
                return;
            };
            if let Some(mut title) = components
                .get_named("title")
                .and_then(|c| text.get_mut(c).ok())
            {
                title.0 = title_text;
            }
            if let Some(mut state) = components
                .get_named("load-state")
                .and_then(|c| text.get_mut(c).ok())
            {
                state.0 = state_text;
            }
        }
        None => {
            let ent = commands
                .spawn(ZOrder::SceneLoadingDialog.default())
                .apply_template(
                    &dui,
                    "out-of-world",
                    DuiProps::new()
                        .with_prop("title", title_text)
                        .with_prop("load-state", state_text)
                        .with_prop("cancel", ChangeRealmDialog::send_default_on::<Click>()),
                )
                .unwrap()
                .root;
            *dialog = Some(ent);
        }
    }
}

#[derive(Component)]
struct LogoPulse;

fn animate_logo_pulse(mut logos: Query<&mut ImageNode, With<LogoPulse>>, time: Res<Time>) {
    for mut img in logos.iter_mut() {
        // ping-pong opacity 0.5..1.0 over 1 second
        let alpha = (0.75 + 0.25 * (time.elapsed_secs_f64() * TAU as f64).sin()) as f32;
        img.color = img.color.with_alpha(alpha);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_loading_backdrop(
    mut commands: Commands,
    oow: Query<&OutOfWorld>,
    mut dialog: Local<Option<Entity>>,
    asset_server: Res<AssetServer>,
) {
    match (oow.is_empty(), dialog.is_some()) {
        (true, true) => {
            // in world, dialog is showing, remove it
            commands.entity(dialog.take().unwrap()).despawn();
        }
        (false, false) => {
            // not in world, dialog is not showing, show it
            let ent = commands
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..Default::default()
                    },
                    BackgroundColor(Color::srgb(0.45, 0.15, 0.55)),
                    ZOrder::OutOfWorldBackdrop.default(),
                ))
                .try_with_children(|parent| {
                    // gradient background
                    parent.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            ..Default::default()
                        },
                        ImageNode::new(
                            asset_server.load_with_settings::<Image, ImageLoaderSettings>(
                                "embedded://images/gradient-background.png",
                                |s| {
                                    s.transfer_priority =
                                        bevy::asset::RenderAssetTransferPriority::Immediate;
                                },
                            ),
                        ),
                    ));
                    // centered logo
                    parent
                        .spawn((Node {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..Default::default()
                        },))
                        .with_child((
                            Node {
                                width: Val::Vh(10.0),
                                height: Val::Vh(10.0),
                                ..Default::default()
                            },
                            ImageNode::new(
                                asset_server.load_with_settings::<Image, ImageLoaderSettings>(
                                    "embedded://images/logo.png",
                                    |s| {
                                        s.transfer_priority =
                                            bevy::asset::RenderAssetTransferPriority::Immediate;
                                    },
                                ),
                            ),
                            LogoPulse,
                        ));
                })
                .id();
            *dialog = Some(ent);
        }
        _ => (),
    }
}

#[allow(clippy::too_many_arguments)]
fn pipe_scene_loading_ui_stream(
    mut requests: EventReader<SystemApi>,
    time: Res<Time>,
    wallet: Res<Wallet>,
    oow: Query<&OutOfWorld>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    scenes: Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
    mut senders: Local<Vec<RpcStreamSender<SceneLoadingUi>>>,
    mut last_state: Local<Option<SceneLoadingUi>>,
    current_realm: Res<CurrentRealm>,
    world_hold: Option<Res<WorldHold>>,
) {
    // Collect new stream subscribers
    senders.extend(requests.read().filter_map(|ev| {
        if let SystemApi::GetSceneLoadingUiStream(sender) = ev {
            Some(sender.clone())
        } else {
            None
        }
    }));

    // Remove closed senders
    senders.retain(|s| !s.is_closed());

    // If no subscribers, nothing to do
    if senders.is_empty() {
        return;
    }

    // Compute current state
    // held for a lobby, the player waits out-of-world but nothing is loading
    let visible = wallet.address().is_some() && !oow.is_empty() && world_hold.is_none();

    let current_state = if let (true, Ok(player)) = (visible, player.single()) {
        let (title, pending_assets) =
            get_scene_loading_info(player, time.elapsed_secs_f64(), &containing_scene, &scenes);
        SceneLoadingUi {
            visible: true,
            realm_connected: current_realm.connected,
            title,
            pending_assets,
        }
    } else {
        SceneLoadingUi {
            visible: false,
            realm_connected: current_realm.connected,
            title: String::new(),
            pending_assets: None,
        }
    };

    // Only send if state changed
    if last_state.as_ref() != Some(&current_state) {
        for sender in senders.iter() {
            let _ = sender.send(current_state.clone());
        }
        *last_state = Some(current_state);
    }
}
