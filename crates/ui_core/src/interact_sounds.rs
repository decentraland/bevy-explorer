use bevy::prelude::*;
use bevy_ecss::PropertyValues;
use common::structs::SystemAudio;

use crate::{dui_utils::DuiFromStr, ui_actions::UiActionSet};

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

fn play_interact_sounds(
    q: Query<(&InteractSounds, &Interaction), Changed<Interaction>>,
    mut writer: EventWriter<SystemAudio>,
) {
    for (sounds, act) in q.iter() {
        match (sounds, act) {
            (
                InteractSounds {
                    press: Some(sound), ..
                },
                Interaction::Pressed,
            )
            | (
                InteractSounds {
                    hover: Some(sound), ..
                },
                Interaction::Hovered,
            ) => {
                println!("{}", sound);
                writer.send(format!("sounds/{}", sound).into());
            }
            _ => (),
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
                .and_then(PropertyValues::string)
                .map(Into::into),
            press: rule
                .properties
                .get("press")
                .and_then(PropertyValues::string)
                .map(Into::into),
        })
    }
}
