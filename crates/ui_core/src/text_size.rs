use bevy::{
    prelude::*,
    text::BreakLineOn,
    window::{PrimaryWindow, WindowResized},
};
use bevy_dui::{DuiRegistry, DuiTemplate};
use bevy_egui::EguiSettings;

use crate::{combo_box::PropsExt, ModifyComponentExt};

pub struct TextSizePlugin;

impl Plugin for TextSizePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_fontsize);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("small-text", TextTemplate(0.015));
    dui.register_template("med-text", TextTemplate(0.03));
    dui.register_template("large-text", TextTemplate(0.06));
}

pub struct TextTemplate(f32);

impl DuiTemplate for TextTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        ctx: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        commands.insert(FontSize(self.0));
        let font = if self.0 < 0.02 {
            ctx.asset_server().load("fonts/NotoSans-Regular.ttf")
        } else {
            ctx.asset_server().load("fonts/NotoSans-Bold.ttf")
        };
        let wrap = props.take_as::<bool>(ctx, "wrap")?.unwrap_or(true);
        commands.modify_component(move |text: &mut Text| {
            text.linebreak_behavior = if wrap {
                BreakLineOn::WordBoundary
            } else {
                BreakLineOn::NoWrap
            };
            for section in &mut text.sections {
                section.style.font = font.clone();
            }
        });

        Ok(Default::default())
    }
}

#[derive(Component)]
pub struct FontSize(pub f32);

pub fn update_fontsize(
    mut q: Query<(&mut Text, Ref<FontSize>)>,
    mut resized: EventReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut egui_settings: ResMut<EguiSettings>,
) {
    let resized = resized.read().last().is_some();
    let Ok(window) = window.get_single() else {
        return;
    };
    let win_size = window.width().min(window.height());
    for (mut text, size) in q.iter_mut().filter(|(_, sz)| resized || sz.is_changed()) {
        for section in &mut text.sections {
            section.style.font_size = win_size * size.0;
        }
    }
    if resized && win_size > 0.0 {
        egui_settings.scale_factor = win_size / 720.0;
    }
}
