use bevy::prelude::*;
use bevy_dui::{DuiCommandsExt, DuiEntities, DuiProps};
use common::structs::PrimaryUser;
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::gltf_container::GltfLoadingCount,
    ContainingScene, OutOfWorld,
};
use ui_core::ui_actions::{Click, EventDefaultExt};
use wallet::Wallet;

use crate::change_realm::ChangeRealmDialog;

pub struct OowUiPlugin;

impl Plugin for OowUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_oow);
    }
}

#[allow(clippy::too_many_arguments)]
fn set_oow(
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

    let scene = containing_scene
        .get_parcel_oow(player)
        .and_then(|scene| scenes.get(scene).ok());
    let title_text = scene
        .map(|(context, _)| context.title.clone())
        .unwrap_or("Scene".to_owned());
    let state_text = scene
        .map(|(_, gltf_count)| {
            gltf_count
                .map(|c| format!("{} assets", c.0))
                .unwrap_or_else(|| "assets".to_owned())
        })
        .unwrap_or("Scene Info".to_owned());

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
                title.sections[0].value = title_text;
            }
            if let Some(mut state) = components
                .get_named("load-state")
                .and_then(|c| text.get_mut(c).ok())
            {
                state.sections[0].value = state_text;
            }
        }
        None => {
            *dialog = Some(
                commands
                    .spawn_template(
                        &dui,
                        "out-of-world",
                        DuiProps::new()
                            .with_prop("title", title_text)
                            .with_prop("load-state", state_text)
                            .with_prop("cancel", ChangeRealmDialog::send_default_on::<Click>()),
                    )
                    .unwrap()
                    .root,
            );
        }
    }
}
