use avatar::{
    avatar_texture::{BoothInstance, PhotoBooth, PROFILE_UI_RENDERLAYER},
    AvatarShape,
};
use bevy::{prelude::*, render::render_resource::Extent3d};
use common::structs::PrimaryUser;

use crate::profile::{SettingsDialog, SettingsTab};

pub struct EmotesSettingsPlugin;

impl Plugin for EmotesSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_emotes_content);
    }
}

#[derive(Component)]
pub struct EmoteSettings {}

#[allow(clippy::type_complexity)]
fn set_emotes_content(
    mut commands: Commands,
    dialog: Query<(
        Entity,
        Option<&BoothInstance>,
        Option<&AvatarShape>,
        Ref<SettingsDialog>,
    )>,
    mut q: Query<(Entity, &SettingsTab, Option<&mut EmoteSettings>), Changed<SettingsTab>>,
    mut booth: PhotoBooth,
    player: Query<&AvatarShape, (Without<SettingsDialog>, With<PrimaryUser>)>,
    mut prev_tab: Local<Option<SettingsTab>>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab, mut _emote_settings) in q.iter_mut() {
        let Ok((settings_entity, maybe_instance, _, _dialog)) = dialog.get_single() else {
            return;
        };

        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Emotes {
            return;
        }

        commands.entity(ent).despawn_descendants();

        let _instance = maybe_instance.cloned().unwrap_or_else(|| {
            let avatar = player.get_single().unwrap();
            let instance = booth.spawn_booth(
                PROFILE_UI_RENDERLAYER,
                avatar.clone(),
                Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                true,
            );
            commands
                .entity(settings_entity)
                .try_insert((instance.clone(), avatar.clone()));
            instance
        });

        // let new_settings;
        // let emote_settings = match emote_settings {
        //     Some(mut settings) => {
        //         // reset cached data
        //         settings.current_list = Vec::default();
        //         settings.into_inner()
        //     }
        //     None => {
        //         let player_shape = &player.get_single().unwrap().0;
        //         let body_shape = player_shape.body_shape.clone().unwrap();
        //         let body_shape_hash = wearable_pointers
        //             .0
        //             .get(&Urn::from_str(&body_shape.to_lowercase()).unwrap())
        //             .unwrap()
        //             .hash()
        //             .unwrap()
        //             .to_owned();

        //         new_settings = WearablesSettings {
        //             body_shape: body_shape.clone(),
        //             current_wearables: player_shape
        //                 .wearables
        //                 .iter()
        //                 .flat_map(|wearable| {
        //                     Urn::from_str(wearable)
        //                         .ok()
        //                         .and_then(|urn| wearable_pointers.0.get(&urn))
        //                         .and_then(WearablePointerResult::hash)
        //                         .and_then(|hash| {
        //                             wearable_metas.0.get(hash).map(|meta| (meta, hash))
        //                         })
        //                         .map(|(meta, hash)| {
        //                             (meta.data.category, (meta.id.to_owned(), hash.to_owned()))
        //                         })
        //                 })
        //                 .chain(std::iter::once((
        //                     WearableCategory::BODY_SHAPE,
        //                     (body_shape, body_shape_hash),
        //                 )))
        //                 .collect(),
        //             ..Default::default()
        //         };
        //         commands.entity(ent).try_insert(new_settings.clone());
        //         &new_settings
        //     }
        // };
    }
}
