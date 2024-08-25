/*use anyhow::anyhow;
use bevy::prelude::*;
use bevy_dui::{DuiRegistry, DuiTemplate};

use crate::{
    combo_box::PropsExt,
    ui_actions::{DataChanged, On},
    Blocker,
};

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
}

pub struct ComboPlugin;

impl Plugin for ComboPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(PostUpdate, update_comboboxen);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("combo-box", DuiComboBoxTemplate);
}

fn update_comboboxen() {}

pub struct DuiComboBoxTemplate;

impl DuiTemplate for DuiComboBoxTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let combobox = ComboBox {
            empty_text: props.take::<String>("empty-text")?.unwrap_or_default(),
            options: props
                .take::<Vec<String>>("options")?
                .ok_or(anyhow!("no options for combobox"))?,
            selected: props.take_as::<isize>(ctx, "selected")?.unwrap_or(-1),
            allow_null: props.take_as::<bool>(ctx, "allow-null")?.unwrap_or(false),
            disabled: props.take_as::<bool>(ctx, "disabled")?.unwrap_or(false),
        };
        commands.insert(combobox);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}
*/
