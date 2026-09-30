use anyhow::anyhow;
use bevy::prelude::*;
use bevy_dui::{DuiRegistry, DuiTemplate};

use crate::ui_actions::{DataChanged, On};

#[derive(Component)]
pub struct ColorPicker {
    pub color: [f32; 3],
}

impl ColorPicker {
    pub fn new_linear(color: Color) -> Self {
        Self {
            color: color.to_linear().to_f32_array()[0..3].try_into().unwrap(),
        }
    }

    pub fn get_linear(&self) -> Color {
        Color::srgb(self.color[0], self.color[1], self.color[2])
    }
}

pub struct ColorPickerPlugin;

impl Plugin for ColorPickerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("color-picker", DuiColorPicker);
}

// display-only swatch; the egui edit widget it used to spawn is gone, and the
// native ui hosting it is superseded by the ui scene
pub struct DuiColorPicker;
impl DuiTemplate for DuiColorPicker {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        _: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let picker = ColorPicker::new_linear(
            props
                .take::<Color>("color")?
                .ok_or(anyhow!("no initial color"))?,
        );
        let swatch = BackgroundColor(picker.get_linear());
        commands.try_insert((picker, swatch));

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.try_insert(onchanged);
        }

        Ok(Default::default())
    }
}
