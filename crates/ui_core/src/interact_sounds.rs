use bevy::prelude::*;
use bevy_ecss::PropertyValues;
use common::{inputs::CommonInputAction, structs::SystemAudio};
use input_manager::InputManager;

use crate::{
    dui_utils::DuiFromStr,
    ui_actions::{UiActionPriority, UiActionSet},
};

#[derive(Component)]
pub struct InteractSounds {
    hover: Option<String>,
    press: Option<String>,
}

pub struct InteractSoundsPlugin;

impl Plugin for InteractSoundsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, play_interact_sounds.before(UiActionSet));
    }
}

#[derive(Component, PartialEq, PartialOrd)]
pub struct LastInteractSound(InteractSound);

#[derive(PartialEq, PartialOrd)]
enum InteractSound {
    Hover,
    Click,
}

#[allow(clippy::type_complexity)]
fn play_interact_sounds(
    mut commands: Commands,
    q: Query<(
        Entity,
        &InteractSounds,
        &Interaction,
        Option<&UiActionPriority>,
        Option<&LastInteractSound>,
    )>,
    mut writer: EventWriter<SystemAudio>,
    input_manager: InputManager,
) {
    for (entity, sounds, act, maybe_priority, maybe_last) in q.iter() {
        if act == &Interaction::None {
            if maybe_last.is_some() {
                commands.entity(entity).remove::<LastInteractSound>();
            }
            continue;
        }

        if maybe_last < Some(&LastInteractSound(InteractSound::Click))
            && input_manager.just_down(
                CommonInputAction::IaPointer,
                maybe_priority.copied().unwrap_or_default().0,
            )
        {
            if let Some(sound) = sounds.press.as_ref() {
                writer.send(format!("sounds/ui/{sound}").into());
            }
            commands
                .entity(entity)
                .try_insert(LastInteractSound(InteractSound::Click));
            continue;
        }

        if maybe_last < Some(&LastInteractSound(InteractSound::Hover)) {
            if let Some(sound) = sounds.hover.as_ref() {
                writer.send(format!("sounds/ui/{sound}").into());
            }
            commands
                .entity(entity)
                .try_insert(LastInteractSound(InteractSound::Hover));
        }
    }
}

impl DuiFromStr for InteractSounds {
    fn from_str(_: &bevy_dui::DuiContext, value: &str) -> Result<Self, anyhow::Error>
    where
        Self: Sized,
    {
        let content = format!("#inline {{{value}}}");
        let ss = bevy_ecss::StyleSheetAsset::parse("", &content);
        let Some(rule) = ss.iter().next() else {
            anyhow::bail!("no rule?");
        };
        Ok(Self {
            hover: rule
                .properties
                .get("hover")
                .and_then(PropertyValues::string),
            press: rule
                .properties
                .get("press")
                .and_then(PropertyValues::string),
        })
    }
}
