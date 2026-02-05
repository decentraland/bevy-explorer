use bevy::{image::ImageLoaderSettings, prelude::*};
use bevy_dui::{DuiEntities, DuiEntityCommandsExt, DuiProps};
use common::{
    rpc::RpcStreamSender,
    structs::{PrimaryUser, ZOrder},
};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::gltf_container::GltfLoadingCount,
    ContainingScene, OutOfWorld,
};
use system_bridge::{NativeUi, SceneLoadingUi, SystemApi};
use ui_core::ui_actions::{Click, EventDefaultExt};
use wallet::Wallet;

use crate::change_realm::ChangeRealmDialog;

/// Extracts scene loading info: (title, pending_assets_count)
fn get_scene_loading_info(
    player: Entity,
    containing_scene: &ContainingScene,
    scenes: &Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
) -> (String, Option<u32>) {
    let scene = containing_scene
        .get_parcel_oow(player)
        .and_then(|scene| scenes.get(scene).ok());

    let title = scene
        .map(|(context, _)| context.title.clone())
        .unwrap_or_else(|| "Scene".to_owned());

    let pending_assets = scene.and_then(|(_, gltf_count)| gltf_count.map(|c| c.0 as u32));

    (title, pending_assets)
}

pub struct OowUiPlugin;

impl Plugin for OowUiPlugin {
    fn build(&self, app: &mut App) {
        if app.world().resource::<NativeUi>().loading_scene {
            app.add_systems(Update, update_loading_scene_dialog);
        } else {
            app.add_systems(Update, update_loading_backdrop);
        }
        app.add_systems(Update, pipe_scene_loading_ui_stream);
    }
}

#[allow(clippy::too_many_arguments)]
fn update_loading_scene_dialog(
    mut commands: Commands,
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

    let (title_text, pending_assets) = get_scene_loading_info(player, &containing_scene, &scenes);
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
                    ImageNode::new(
                        asset_server.load_with_settings::<Image, ImageLoaderSettings>(
                            "embedded://images/gradient-background.png",
                            |s| {
                                s.transfer_priority =
                                    bevy::asset::RenderAssetTransferPriority::Immediate;
                            },
                        ),
                    ),
                    ZOrder::OutOfWorldBackdrop.default(),
                ))
                .id();
            *dialog = Some(ent);
        }
        _ => (),
    }
}

#[allow(clippy::too_many_arguments)]
fn pipe_scene_loading_ui_stream(
    mut requests: EventReader<SystemApi>,
    wallet: Res<Wallet>,
    oow: Query<&OutOfWorld>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    scenes: Query<(&RendererSceneContext, Option<&GltfLoadingCount>)>,
    mut senders: Local<Vec<RpcStreamSender<SceneLoadingUi>>>,
    mut last_state: Local<Option<SceneLoadingUi>>,
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
    let visible = wallet.address().is_some() && !oow.is_empty();

    let current_state = if let (true, Ok(player)) = (visible, player.single()) {
        let (title, pending_assets) = get_scene_loading_info(player, &containing_scene, &scenes);
        SceneLoadingUi {
            visible: true,
            title,
            pending_assets,
        }
    } else {
        SceneLoadingUi {
            visible: false,
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
