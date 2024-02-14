use avatar::avatar_texture::BoothInstance;
use bevy::prelude::*;

use crate::profile2::{SettingsDialog, SettingsTab};

pub struct EmotesSettingsPlugin;

impl Plugin for EmotesSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_emotes_content);
    }
}

fn set_emotes_content(
    mut commands: Commands,
    dialog: Query<(Entity, Option<&BoothInstance>), With<SettingsDialog>>,
    mut q: Query<(Entity, &SettingsTab), Changed<SettingsTab>>,
    mut prev_tab: Local<Option<SettingsTab>>,
) {
    if dialog.is_empty() {
        *prev_tab = None;
    }

    for (ent, tab) in q.iter_mut() {
        if *prev_tab == Some(*tab) {
            continue;
        }
        *prev_tab = Some(*tab);

        if tab != &SettingsTab::Emotes {
            return;
        }

        commands.entity(ent).despawn_descendants();
    }
}
